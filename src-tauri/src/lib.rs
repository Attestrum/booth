mod audio;
mod config;
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
fn get_recents(app: tauri::AppHandle) -> Vec<String> {
    config::load(&app)
        .recents
        .into_iter()
        .filter(|r| Path::new(r).is_dir())
        .collect()
}

#[tauri::command]
fn add_project(app: tauri::AppHandle, dir: String) -> Result<Vec<String>, String> {
    if !Path::new(&dir).is_dir() {
        return Err(format!("not a folder: {dir}"));
    }
    config::add_recent(&app, &dir)
        .map(|c| c.recents)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn scan_sessions(root: String) -> Vec<SessionSummary> {
    session::scan(Path::new(&root))
}

#[tauri::command]
fn list_episodes(root: String) -> Vec<String> {
    session::list_candidates(Path::new(&root))
}

#[tauri::command]
fn open_episode(
    app: tauri::AppHandle,
    dir: String,
    now_iso: String,
) -> Result<OpenResult, String> {
    let path = Path::new(&dir);
    // playback streams take WAVs through the asset protocol; access is granted
    // per opened folder at runtime — there is no static scope in tauri.conf.json
    app.asset_protocol_scope()
        .allow_directory(path, true)
        .map_err(|e| e.to_string())?;
    let (session, fresh) = session::open(path, now_iso).map_err(|e| format!("{e:#}"))?;
    Ok(OpenResult { session, fresh })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    dir: String,
    session: Session,
    fresh: bool,
}

/// Import a .md / .txt script: parse it to units, persist them as
/// narration/script-units.json in the script's folder (the standard parse
/// ladder + session machinery take over from there), link the session's
/// sourceFile for inline-edit write-back, and remember the folder.
#[tauri::command]
fn import_script(
    app: tauri::AppHandle,
    path: String,
    now_iso: String,
) -> Result<ImportResult, String> {
    let src = PathBuf::from(&path);
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let markdown = matches!(ext.as_str(), "md" | "markdown");
    if !markdown && ext != "txt" {
        return Err(format!(
            "can't import .{ext} — bring a .md or .txt script (PDF/docx: export to one of those first)"
        ));
    }
    let dir = src
        .parent()
        .ok_or("script file has no parent folder")?
        .to_path_buf();
    if session::scan(&dir).iter().any(|s| s.episode_dir == dir.to_string_lossy()) {
        return Err(
            "this folder already has a booth session — open it from the list instead \
             (or remove its narration/booth/ to re-import)"
                .into(),
        );
    }
    let raw = std::fs::read_to_string(&src).map_err(|e| format!("read {path}: {e}"))?;
    let units = script::units_from_document(&raw, markdown);
    if units.is_empty() {
        return Err("no readable text found in that file".into());
    }
    std::fs::create_dir_all(dir.join("narration")).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("narration/script-units.json"),
        serde_json::to_string_pretty(&units).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    app.asset_protocol_scope()
        .allow_directory(&dir, true)
        .map_err(|e| e.to_string())?;
    let (mut session, fresh) =
        session::open(&dir, now_iso).map_err(|e| format!("{e:#}"))?;
    session.source_file = Some(path.clone());
    session::save(&dir, &session).map_err(|e| format!("{e:#}"))?;
    let _ = config::add_recent(&app, &dir.to_string_lossy());

    Ok(ImportResult {
        dir: dir.to_string_lossy().into_owned(),
        session,
        fresh,
    })
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
    mp3: Option<String>,
}

/// Whether mp3 encode / mixed-rate resampling are available (Review screen
/// shows the status before export).
#[tauri::command]
fn ffmpeg_status() -> bool {
    export::ffmpeg_available().is_some()
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
        mp3: mp3.map(|p| p.to_string_lossy().into_owned()),
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
            get_recents,
            add_project,
            scan_sessions,
            list_episodes,
            open_episode,
            import_script,
            save_session,
            start_recording,
            stop_recording,
            discard_take,
            discard_take_at,
            edit_unit_text,
            undo_discard,
            take_path,
            ffmpeg_status,
            export_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
