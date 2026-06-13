//! Decode any supported audio/video container to the 16 kHz mono f32 PCM that
//! whisper.cpp expects. WAV goes through the existing `hound` dependency;
//! everything else (mp3 / m4a / mp4 / mov audio track / aac / alac) through
//! `symphonia`. Resampling to 16 kHz is done with `rubato` (high-quality sinc).

use anyhow::{anyhow, bail, Result};
use std::path::Path;

const TARGET_RATE: u32 = 16_000;

/// Decode `path` to mono 16 kHz f32 samples, returning the samples and the
/// source duration in seconds (computed before resampling).
pub fn decode_to_mono_16k(path: &Path) -> Result<(Vec<f32>, f64)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let (mono, rate) = if ext == "wav" {
        decode_wav(path)?
    } else {
        decode_symphonia(path)?
    };
    if mono.is_empty() || rate == 0 {
        bail!("decoded no audio from {}", path.display());
    }
    let duration_sec = mono.len() as f64 / rate as f64;
    let out = resample_to_16k(&mono, rate)?;
    Ok((out, duration_sec))
}

fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|f| f.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn decode_wav(path: &Path) -> Result<(Vec<f32>, u32)> {
    let mut rd = hound::WavReader::open(path)?;
    let spec = rd.spec();
    let channels = spec.channels.max(1) as usize;
    let mono = match spec.sample_format {
        hound::SampleFormat::Float => {
            let s: Vec<f32> = rd.samples::<f32>().filter_map(Result::ok).collect();
            downmix(&s, channels)
        }
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            let s: Vec<f32> = rd
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|v| v as f32 / max)
                .collect();
            downmix(&s, channels)
        }
    };
    Ok((mono, spec.sample_rate))
}

fn decode_symphonia(path: &Path) -> Result<(Vec<f32>, u32)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::errors::Error as SymError;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("no decodable audio track in {}", path.display()))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

    let mut rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut mono: Vec<f32> = Vec::new();
    let mut sbuf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => break, // end of stream / reset
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                if rate == 0 {
                    rate = spec.rate;
                }
                let channels = spec.channels.count().max(1);
                if sbuf.is_none() {
                    sbuf = Some(SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
                }
                let sb = sbuf.as_mut().unwrap();
                sb.copy_interleaved_ref(decoded);
                for frame in sb.samples().chunks(channels) {
                    mono.push(frame.iter().sum::<f32>() / channels as f32);
                }
            }
            Err(SymError::DecodeError(_)) => continue, // skip a bad packet
            Err(_) => break,
        }
    }
    Ok((mono, rate))
}

fn resample_to_16k(input: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    if from_rate == TARGET_RATE {
        return Ok(input.to_vec());
    }
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let ratio = TARGET_RATE as f64 / from_rate as f64;
    let chunk = 1024usize;
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, chunk, 1)
        .map_err(|e| anyhow!("rubato init: {e}"))?;

    let mut out: Vec<f32> = Vec::with_capacity((input.len() as f64 * ratio) as usize + 64);
    let mut idx = 0;
    while idx < input.len() {
        let need = resampler.input_frames_next();
        if idx + need <= input.len() {
            let res = resampler
                .process(&[&input[idx..idx + need]], None)
                .map_err(|e| anyhow!("rubato process: {e}"))?;
            out.extend_from_slice(&res[0]);
            idx += need;
        } else {
            // final short chunk
            let res = resampler
                .process_partial(Some(&[&input[idx..]]), None)
                .map_err(|e| anyhow!("rubato tail: {e}"))?;
            out.extend_from_slice(&res[0]);
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn write_sine_wav(path: &Path, rate: u32, channels: u16, secs: f32) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (rate as f32 * secs) as usize;
        for i in 0..n {
            let v = (2.0 * PI * 440.0 * i as f32 / rate as f32).sin();
            let s = (v * 16000.0) as i16;
            for _ in 0..channels {
                w.write_sample(s).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    #[test]
    fn wav_44k_stereo_decodes_and_resamples_to_16k_mono() {
        let dir = std::env::temp_dir();
        let p = dir.join("booth_decode_test_44k.wav");
        write_sine_wav(&p, 44_100, 2, 1.0);
        let (samples, dur) = decode_to_mono_16k(&p).unwrap();
        // ~1 second at 16 kHz, within a small resampler margin
        assert!((dur - 1.0).abs() < 0.05, "duration {dur}");
        assert!(
            (samples.len() as i64 - 16_000).abs() < 512,
            "got {} samples",
            samples.len()
        );
        // sine stays bounded
        assert!(samples.iter().all(|s| s.abs() <= 1.01));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn wav_already_16k_passes_through() {
        let p = std::env::temp_dir().join("booth_decode_test_16k.wav");
        write_sine_wav(&p, 16_000, 1, 0.5);
        let (samples, _) = decode_to_mono_16k(&p).unwrap();
        assert_eq!(samples.len(), 8_000);
        let _ = std::fs::remove_file(&p);
    }
}
