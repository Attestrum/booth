// Microphone capture on a dedicated thread (cpal Stream is !Send): the webview
// never touches the mic. CoreAudio callback -> 24-bit WAV via hound (header kept
// valid by 30 Hz flushes), plus 30 Hz level/window frames emitted to the UI.
use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use serde::Serialize;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const FRAME_WINDOW: usize = 128;
const I24_MAX: f32 = 8_388_607.0;

#[derive(Clone, Serialize)]
pub struct AudioFrame {
    pub rms: f32,
    pub peak: f32,
    pub clip: bool,
    pub window: Vec<f32>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    pub sample_rate: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopInfo {
    pub duration_sec: f64,
    pub sample_rate: u32,
}

/// Shared between the CoreAudio callback and the engine thread.
struct Capture {
    writer: hound::WavWriter<BufWriter<File>>,
    /// mono samples since the last UI frame
    pending: Vec<f32>,
    clipped: bool,
}

enum Cmd {
    Start {
        path: PathBuf,
        reply: Sender<Result<u32, String>>,
    },
    Stop {
        reply: Sender<Result<StopInfo, String>>,
    },
}

pub struct AudioEngine {
    tx: Sender<Cmd>,
}

impl AudioEngine {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = unbounded::<Cmd>();
        std::thread::spawn(move || engine_thread(app, rx));
        Self { tx }
    }

    pub fn start(&self, path: PathBuf) -> Result<u32, String> {
        let (reply, get) = bounded(1);
        self.tx
            .send(Cmd::Start { path, reply })
            .map_err(|e| e.to_string())?;
        get.recv().map_err(|e| e.to_string())?
    }

    pub fn stop(&self) -> Result<StopInfo, String> {
        let (reply, get) = bounded(1);
        self.tx
            .send(Cmd::Stop { reply })
            .map_err(|e| e.to_string())?;
        get.recv().map_err(|e| e.to_string())?
    }
}

pub fn current_device() -> Result<DeviceInfo, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("no input device available")?;
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    Ok(DeviceInfo {
        name: device.name().unwrap_or_else(|_| "unknown".into()),
        sample_rate: config.sample_rate().0,
    })
}

fn engine_thread(app: AppHandle, rx: Receiver<Cmd>) {
    // stream + capture state for the take in flight (stream must live on this thread)
    let mut active: Option<(cpal::Stream, Arc<Mutex<Capture>>, u32)> = None;

    loop {
        match rx.recv_timeout(Duration::from_millis(33)) {
            Ok(Cmd::Start { path, reply }) => {
                if active.is_some() {
                    let _ = reply.send(Err("already recording".into()));
                    continue;
                }
                match open_stream(&path) {
                    Ok((stream, capture, rate)) => {
                        active = Some((stream, capture, rate));
                        let _ = reply.send(Ok(rate));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e.to_string()));
                    }
                }
            }
            Ok(Cmd::Stop { reply }) => match active.take() {
                Some((stream, capture, rate)) => {
                    drop(stream); // stops the CoreAudio callback
                    let result = Arc::try_unwrap(capture)
                        .map_err(|_| anyhow!("capture still referenced"))
                        .and_then(|m| {
                            let cap = m.into_inner().map_err(|_| anyhow!("poisoned"))?;
                            let frames = cap.writer.duration() as f64;
                            cap.writer.finalize()?;
                            Ok(StopInfo {
                                duration_sec: frames / rate as f64,
                                sample_rate: rate,
                            })
                        });
                    let _ = reply.send(result.map_err(|e| e.to_string()));
                }
                None => {
                    let _ = reply.send(Err("not recording".into()));
                }
            },
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                // 30 Hz tick: emit a UI frame + keep the WAV header valid (crash safety)
                if let Some((_, capture, _)) = &active {
                    let mut cap = capture.lock().unwrap();
                    let frame = make_frame(&mut cap);
                    let _ = cap.writer.flush();
                    drop(cap);
                    let _ = app.emit("audio:frame", frame);
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn make_frame(cap: &mut Capture) -> AudioFrame {
    let samples = std::mem::take(&mut cap.pending);
    let n = samples.len().max(1);
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / n as f32).sqrt();
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    // downsample to a fixed window for the oscilloscope
    let window: Vec<f32> = if samples.is_empty() {
        vec![0.0; FRAME_WINDOW]
    } else {
        (0..FRAME_WINDOW)
            .map(|i| samples[i * samples.len() / FRAME_WINDOW])
            .collect()
    };
    let clip = cap.clipped;
    cap.clipped = false;
    AudioFrame { rms, peak, clip, window }
}

fn open_stream(path: &PathBuf) -> Result<(cpal::Stream, Arc<Mutex<Capture>>, u32)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no input device available"))?;
    let default = device.default_input_config()?;

    // prefer 48 kHz if the device supports it at its default sample format
    let mut sample_rate = default.sample_rate();
    if let Ok(ranges) = device.supported_input_configs() {
        for r in ranges {
            if r.sample_format() == default.sample_format()
                && r.min_sample_rate().0 <= 48_000
                && r.max_sample_rate().0 >= 48_000
            {
                sample_rate = cpal::SampleRate(48_000);
                break;
            }
        }
    }
    let config = cpal::StreamConfig {
        channels: default.channels(),
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };
    let channels = config.channels as usize;
    let rate = config.sample_rate.0;

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let writer = hound::WavWriter::create(path, spec)?;
    let capture = Arc::new(Mutex::new(Capture {
        writer,
        pending: Vec::with_capacity(8192),
        clipped: false,
    }));

    let cb_capture = Arc::clone(&capture);
    let err_fn = |e| eprintln!("audio stream error: {e}");

    let stream = match default.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| write_mono(&cb_capture, data, channels, |s| s),
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                write_mono(&cb_capture, data, channels, |s| s as f32 / 32768.0)
            },
            err_fn,
            None,
        )?,
        f => return Err(anyhow!("unsupported sample format {f}")),
    };
    stream.play()?;
    Ok((stream, capture, rate))
}

/// Downmix interleaved input to mono, write 24-bit samples, stash for UI frames.
fn write_mono<T: Copy>(
    capture: &Arc<Mutex<Capture>>,
    data: &[T],
    channels: usize,
    to_f32: impl Fn(T) -> f32,
) {
    let mut cap = capture.lock().unwrap();
    for frame in data.chunks_exact(channels) {
        let mono = frame.iter().map(|&s| to_f32(s)).sum::<f32>() / channels as f32;
        if mono.abs() >= 0.999 {
            cap.clipped = true;
        }
        let _ = cap
            .writer
            .write_sample((mono.clamp(-1.0, 1.0) * I24_MAX) as i32);
        cap.pending.push(mono);
    }
}
