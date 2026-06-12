mod audio;
mod export;
mod script;
mod session;
mod wav;

use audio::{AudioEngine, DeviceInfo};
use serde::Serialize;
use session::{Session, SessionSummary, Take};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// This is a personal tool for one workspace; episodes always live here.
fn episodes_root() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    Path::new(&home).join("dev/Attestrum-youtube/episodes")
}

/// The take in flight: (episode_dir, passage, wav path). One at a time.
struct RecordingState(Mutex<Option<(PathBuf, usize, PathBuf)>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenResult {
    session: Session,
    fresh: bool,
}

#[tauri::command]
fn current_device() -> Result<DeviceInfo, String> {
    audio::current_device()
}

#[tauri::command]
fn scan_sessions() -> Vec<SessionSummary> {
    session::scan(&episodes_root())
}

#[tauri::command]
fn list_episodes() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(episodes_root()) else {
        return Vec::new();
    };
    let mut dirs: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path().to_string_lossy().into_owned())
        .collect();
    dirs.sort();
    dirs
}

#[tauri::command]
fn open_episode(dir: String, now_iso: String) -> Result<OpenResult, String> {
    let (session, fresh) =
        session::open(Path::new(&dir), now_iso).map_err(|e| format!("{e:#}"))?;
    Ok(OpenResult { session, fresh })
}

#[tauri::command]
fn save_session(dir: String, session: Session) -> Result<(), String> {
    session::save(Path::new(&dir), &session).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn start_recording(
    engine: tauri::State<'_, AudioEngine>,
    rec: tauri::State<'_, RecordingState>,
    dir: String,
    passage: usize,
) -> Result<String, String> {
    let episode_dir = PathBuf::from(&dir);
    let s = session::load(&episode_dir).map_err(|e| format!("{e:#}"))?;

    // Mixed sample rates within a session are ALLOWED (rate gate removed,
    // founder 2026-06-12) — export resamples minority-rate takes before concat.

    let mut state = rec.0.lock().unwrap();
    if state.is_some() {
        return Err("already recording".into());
    }
    let path = session::next_take_path(&episode_dir, &s, passage).map_err(|e| format!("{e:#}"))?;
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    engine.start(path.clone())?;
    *state = Some((episode_dir, passage, path));
    Ok(name)
}

#[tauri::command]
fn stop_recording(
    engine: tauri::State<'_, AudioEngine>,
    rec: tauri::State<'_, RecordingState>,
) -> Result<Session, String> {
    let (episode_dir, passage, path) = rec
        .0
        .lock()
        .unwrap()
        .take()
        .ok_or("not recording")?;
    let info = engine.stop()?;

    let mut s = session::load(&episode_dir).map_err(|e| format!("{e:#}"))?;
    // informational only (top-rail display) — tracks the LATEST take's format
    s.format = Some(session::AudioFormat {
        sample_rate: info.sample_rate,
        channels: 1,
        bits: 24,
    });
    s.device = audio::current_device().ok().map(|d| d.name);
    s.passages
        .get_mut(passage)
        .ok_or("passage out of range")?
        .takes
        .push(Take {
            file: path.file_name().unwrap().to_string_lossy().into_owned(),
            duration_sec: info.duration_sec,
            recovered: false,
        });
    session::save(&episode_dir, &s).map_err(|e| format!("{e:#}"))?;
    Ok(s)
}

#[tauri::command]
fn discard_take(dir: String, passage: usize) -> Result<(Session, Take), String> {
    let episode_dir = PathBuf::from(&dir);
    let mut s = session::load(&episode_dir).map_err(|e| format!("{e:#}"))?;
    let take = session::discard_top(&episode_dir, &mut s, passage).map_err(|e| format!("{e:#}"))?;
    Ok((s, take))
}

#[tauri::command]
fn discard_take_at(
    dir: String,
    passage: usize,
    index: usize,
) -> Result<(Session, Take), String> {
    let episode_dir = PathBuf::from(&dir);
    let mut s = session::load(&episode_dir).map_err(|e| format!("{e:#}"))?;
    let take =
        session::discard_at(&episode_dir, &mut s, passage, index).map_err(|e| format!("{e:#}"))?;
    Ok((s, take))
}

#[tauri::command]
fn edit_unit_text(
    dir: String,
    unit: usize,
    text: String,
) -> Result<(Session, Vec<String>), String> {
    let episode_dir = PathBuf::from(&dir);
    let mut s = session::load(&episode_dir).map_err(|e| format!("{e:#}"))?;
    let warnings =
        session::edit_unit_text(&episode_dir, &mut s, unit, text).map_err(|e| format!("{e:#}"))?;
    Ok((s, warnings))
}

#[tauri::command]
fn undo_discard(dir: String, passage: usize, take: Take) -> Result<Session, String> {
    let episode_dir = PathBuf::from(&dir);
    let mut s = session::load(&episode_dir).map_err(|e| format!("{e:#}"))?;
    session::undo_discard(&episode_dir, &mut s, passage, take).map_err(|e| format!("{e:#}"))?;
    Ok(s)
}

#[derive(Serialize)]
struct ExportResult {
    wav: String,
    mp3: String,
}

#[tauri::command]
fn export_session(
    app: tauri::AppHandle,
    dir: String,
    allow_partial: bool,
) -> Result<ExportResult, String> {
    let episode_dir = PathBuf::from(&dir);
    let s = session::load(&episode_dir).map_err(|e| format!("{e:#}"))?;
    let (wav, mp3) = export::export(&app, &episode_dir, &s, allow_partial)
        .map_err(|e| format!("{e:#}"))?;
    Ok(ExportResult {
        wav: wav.to_string_lossy().into_owned(),
        mp3: mp3.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
fn take_path(dir: String, file: String) -> String {
    session::takes_dir(Path::new(&dir))
        .join(file)
        .to_string_lossy()
        .into_owned()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let engine = AudioEngine::new(app.handle().clone());
            app.manage(engine);
            app.manage(RecordingState(Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            current_device,
            scan_sessions,
            list_episodes,
            open_episode,
            save_session,
            start_recording,
            stop_recording,
            discard_take,
            discard_take_at,
            edit_unit_text,
            undo_discard,
            take_path,
            export_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
