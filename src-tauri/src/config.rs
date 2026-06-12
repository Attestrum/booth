// App-level state: the project folders Booth has opened (newest first,
// deduped, capped), persisted as config.json in the OS app-config dir.
// This replaces the original hardcoded episodes root — any folder can be
// a project now.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

const MAX_RECENTS: usize = 8;

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub recents: Vec<String>,
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .context("no app config dir on this platform")?;
    fs::create_dir_all(&dir)?;
    Ok(dir.join("config.json"))
}

pub fn load(app: &tauri::AppHandle) -> AppConfig {
    config_path(app)
        .and_then(|p| Ok(serde_json::from_str(&fs::read_to_string(p)?)?))
        .unwrap_or_default()
}

/// Move (or insert) `dir` to the front of the recents list and persist.
pub fn add_recent(app: &tauri::AppHandle, dir: &str) -> Result<AppConfig> {
    let mut cfg = load(app);
    cfg.recents.retain(|r| r != dir);
    cfg.recents.insert(0, dir.to_string());
    cfg.recents.truncate(MAX_RECENTS);
    fs::write(config_path(app)?, serde_json::to_string_pretty(&cfg)?)?;
    Ok(cfg)
}
