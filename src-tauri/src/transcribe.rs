//! The transcription engine: a dedicated worker thread (mirrors `AudioEngine`)
//! that loads the heavy `WhisperContext` once and keeps it resident, then runs
//! the Diagram-2 pipeline per job — probe captions → import | download → decode
//! → whisper → save — streaming `transcribe:progress` events and emitting
//! `transcribe:done` (the saved `Transcript`) or `transcribe:error`. Downloaded
//! media lives in a scratch dir deleted on every exit (success or failure).

use crate::decode::decode_to_mono_16k;
use crate::transcript::{Segment, SegmentSource, SourceKind, Transcript};
use crate::whisper::Whisper;
use crate::ytdlp::{CaptionKind, YtDlp};
use crate::{model, subtitles, transcripts, ytdlp};
use anyhow::{bail, Context, Result};
use crossbeam_channel::{unbounded, Sender};
use std::path::{Path, PathBuf};
use tauri::{path::BaseDirectory, AppHandle, Emitter, Manager};

/// What to transcribe.
pub enum Source {
    Url(String),
    File(PathBuf),
}

struct Job {
    source: Source,
    now_iso: String,
}

/// Handle to the worker thread; lives in Tauri state.
pub struct TranscriptionEngine {
    tx: Sender<Job>,
}

impl TranscriptionEngine {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = unbounded::<Job>();
        std::thread::spawn(move || {
            // Loaded lazily on the first Whisper job and kept resident.
            let mut whisper: Option<Whisper> = None;
            for job in rx {
                match run_job(&app, &mut whisper, &job) {
                    Ok(t) => {
                        let _ = app.emit("transcribe:done", &t);
                    }
                    Err(e) => {
                        let _ = app.emit("transcribe:error", format!("{e:#}"));
                    }
                }
            }
        });
        Self { tx }
    }

    /// Queue a job (returns immediately; results arrive as events).
    pub fn submit(&self, source: Source, now_iso: String) -> std::result::Result<(), String> {
        self.tx
            .send(Job { source, now_iso })
            .map_err(|e| e.to_string())
    }
}

/// Fields a pipeline branch produces (everything but id/created_at).
struct Built {
    title: String,
    source: String,
    source_kind: SourceKind,
    segment_source: SegmentSource,
    model: Option<String>,
    language: Option<String>,
    duration_sec: f64,
    segments: Vec<Segment>,
}

fn run_job(app: &AppHandle, whisper: &mut Option<Whisper>, job: &Job) -> Result<Transcript> {
    let app_data = app.path().app_data_dir().context("resolve app data dir")?;
    let id = gen_id();
    let scratch = Scratch(std::env::temp_dir().join(format!("booth-tx-{id}")));
    std::fs::create_dir_all(&scratch.0)?;

    let built = match &job.source {
        Source::File(path) => build_from_file(app, whisper, &app_data, path)?,
        Source::Url(url) => build_from_url(app, whisper, &app_data, &scratch.0, url)?,
    };
    if built.segments.is_empty() {
        bail!("no speech detected");
    }

    let t = Transcript {
        id,
        title: built.title,
        source: built.source,
        source_kind: built.source_kind,
        segment_source: built.segment_source,
        model: built.model,
        language: built.language,
        created_at: job.now_iso.clone(),
        duration_sec: built.duration_sec,
        segments: built.segments,
    };
    emit(app, "SAVING", None);
    transcripts::save(&app_data, &t)?;
    Ok(t)
    // `scratch` drops here → any downloaded media is deleted
}

fn build_from_file(
    app: &AppHandle,
    whisper: &mut Option<Whisper>,
    app_data: &Path,
    path: &Path,
) -> Result<Built> {
    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "transcript".into());
    let (segments, duration_sec, model) = whisper_pipeline(app, whisper, app_data, path)?;
    Ok(Built {
        title,
        source: path.to_string_lossy().to_string(),
        source_kind: SourceKind::File,
        segment_source: SegmentSource::Whisper,
        model,
        language: None,
        duration_sec,
        segments,
    })
}

fn build_from_url(
    app: &AppHandle,
    whisper: &mut Option<Whisper>,
    app_data: &Path,
    scratch: &Path,
    url: &str,
) -> Result<Built> {
    let ydl = YtDlp::new();

    // TikTok/Instagram/Facebook rarely expose a usable caption track, so skip
    // the slow probe and go straight to audio + Whisper. The download captures
    // the title/duration that the caption pass would otherwise have supplied.
    if ytdlp::skips_caption_probe(url) {
        emit(app, "DOWNLOADING AUDIO", None);
        let (audio, title, duration) = ydl.download_audio_with_meta(url, scratch)?;
        let (segments, duration_sec, model) = whisper_pipeline(app, whisper, app_data, &audio)?;
        return Ok(Built {
            title,
            source: url.to_string(),
            source_kind: SourceKind::Url,
            segment_source: SegmentSource::Whisper,
            model,
            language: None,
            duration_sec: if duration > 0.0 { duration } else { duration_sec },
            segments,
        });
    }

    emit(app, "PROBING CAPTIONS", None);
    let fetched = ydl.fetch_captions(url, scratch, &["en"])?;

    if let Some(cap) = fetched.caption {
        let (label, seg_source) = match cap.kind {
            CaptionKind::Manual => ("manual-subs", SegmentSource::ManualSubs),
            CaptionKind::Auto => ("auto-subs", SegmentSource::AutoSubs),
        };
        emit(app, &format!("CAPTIONS FOUND ▸ importing {label}"), None);
        let raw = std::fs::read_to_string(&cap.file)?;
        let segments = subtitles::parse(&raw);
        if segments.is_empty() {
            bail!("caption file had no readable cues");
        }
        let tail = segments.last().map(|s| s.end_ms as f64 / 1000.0).unwrap_or(0.0);
        Ok(Built {
            title: fetched.title,
            source: url.to_string(),
            source_kind: SourceKind::Url,
            segment_source: seg_source,
            model: None,
            language: Some(cap.lang),
            duration_sec: if fetched.duration_sec > 0.0 { fetched.duration_sec } else { tail },
            segments,
        })
    } else {
        emit(app, "DOWNLOADING AUDIO", None);
        let audio = ydl.download_audio(url, scratch)?;
        let (segments, duration_sec, model) = whisper_pipeline(app, whisper, app_data, &audio)?;
        Ok(Built {
            title: fetched.title,
            source: url.to_string(),
            source_kind: SourceKind::Url,
            segment_source: SegmentSource::Whisper,
            model,
            language: None,
            duration_sec: if fetched.duration_sec > 0.0 { fetched.duration_sec } else { duration_sec },
            segments,
        })
    }
}

/// model present? → load once → decode → resample → whisper.
fn whisper_pipeline(
    app: &AppHandle,
    whisper: &mut Option<Whisper>,
    app_data: &Path,
    audio: &Path,
) -> Result<(Vec<Segment>, f64, Option<String>)> {
    if !model::is_present(app_data) {
        emit(app, "DOWNLOADING MODEL", Some(0));
        let app2 = app.clone();
        model::ensure_model(app_data, move |pct| emit(&app2, "DOWNLOADING MODEL", Some(pct)))?;
    }
    if whisper.is_none() {
        emit(app, "LOADING MODEL", None);
        *whisper = Some(Whisper::load(&model::model_path(app_data))?);
    }

    emit(app, "DECODING", None);
    let (pcm, duration_sec) = decode_to_mono_16k(audio)?;

    // Bundled Silero VAD model (864 KB resource). Resolve once per job; if it's
    // missing we transcribe without VAD rather than fail.
    let vad_path = app
        .path()
        .resolve("resources/models/ggml-silero-v6.2.0.bin", BaseDirectory::Resource)
        .ok()
        .filter(|p| p.exists());
    if vad_path.is_none() {
        eprintln!("transcribe: Silero VAD model not found; transcribing without VAD");
    }
    let vad = vad_path.as_deref().and_then(|p| p.to_str());

    emit(app, "TRANSCRIBING", Some(0));
    let app3 = app.clone();
    let segments = whisper
        .as_ref()
        .unwrap()
        .transcribe(&pcm, None, vad, move |p| {
            emit(&app3, "TRANSCRIBING", Some(p.clamp(0, 100) as u8))
        })?;

    let model_id = model::MODEL_FILE
        .trim_start_matches("ggml-")
        .trim_end_matches(".bin")
        .to_string();
    Ok((segments, duration_sec, Some(model_id)))
}

fn emit(app: &AppHandle, stage: &str, pct: Option<u8>) {
    let _ = app.emit(
        "transcribe:progress",
        serde_json::json!({ "stage": stage, "pct": pct }),
    );
}

/// Unique id from the wall clock (monotonic enough for filenames; collisions
/// across nanoseconds are not a concern for a single local user).
fn gen_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("t{nanos:x}")
}

/// Deletes its directory on drop — guarantees transient media never lingers.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
