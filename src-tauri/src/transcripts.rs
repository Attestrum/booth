//! The saved-transcript library: one `<id>.json` per transcript under
//! `<app_data>/transcripts/`. Re-openable from the Load screen's TRANSCRIPTS
//! group. Pure filesystem + serde — no Tauri, so it unit-tests directly.

use crate::transcript::{SegmentSource, SourceKind, Transcript};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Library directory.
pub fn dir(app_data: &Path) -> PathBuf {
    app_data.join("transcripts")
}

/// Lightweight row for the Load-screen list (no segments).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub source: String,
    pub source_kind: SourceKind,
    pub segment_source: SegmentSource,
    pub model: Option<String>,
    pub created_at: String,
    pub duration_sec: f64,
    pub n_segments: usize,
}

/// Persist a transcript (atomic temp+rename).
pub fn save(app_data: &Path, t: &Transcript) -> Result<()> {
    let id = safe_id(&t.id)?;
    let d = dir(app_data);
    std::fs::create_dir_all(&d)?;
    let path = d.join(format!("{id}.json"));
    let tmp = d.join(format!("{id}.json.part"));
    std::fs::write(&tmp, serde_json::to_string_pretty(t)?)?;
    std::fs::rename(&tmp, &path).context("finalize transcript")?;
    Ok(())
}

/// Absolute path of a transcript's JSON file (id-validated).
pub fn path(app_data: &Path, id: &str) -> Result<PathBuf> {
    Ok(dir(app_data).join(format!("{}.json", safe_id(id)?)))
}

/// Load the full transcript by id.
pub fn load(app_data: &Path, id: &str) -> Result<Transcript> {
    let id = safe_id(id)?;
    let path = dir(app_data).join(format!("{id}.json"));
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(serde_json::from_str(&raw)?)
}

/// Delete a transcript (no error if already gone).
pub fn delete(app_data: &Path, id: &str) -> Result<()> {
    let id = safe_id(id)?;
    let path = dir(app_data).join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// All transcripts, newest first.
pub fn list(app_data: &Path) -> Vec<Summary> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir(app_data)) else {
        return out;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(t) = serde_json::from_str::<Transcript>(&raw) {
                    out.push(Summary {
                        id: t.id,
                        title: t.title,
                        source: t.source,
                        source_kind: t.source_kind,
                        segment_source: t.segment_source,
                        model: t.model,
                        created_at: t.created_at,
                        duration_sec: t.duration_sec,
                        n_segments: t.segments.len(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

/// Reject ids that could escape the library directory.
fn safe_id(id: &str) -> Result<&str> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid transcript id");
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Segment;

    fn sample(id: &str, created: &str) -> Transcript {
        Transcript {
            id: id.into(),
            title: format!("T {id}"),
            source: "x".into(),
            source_kind: SourceKind::File,
            segment_source: SegmentSource::Whisper,
            model: Some("large-v3-turbo".into()),
            language: None,
            created_at: created.into(),
            duration_sec: 1.0,
            segments: vec![Segment { start_ms: 0, end_ms: 1, text: "hi".into() }],
        }
    }

    #[test]
    fn save_list_load_delete_round_trip() {
        let base = std::env::temp_dir().join("booth_transcripts_test");
        let _ = std::fs::remove_dir_all(&base);

        save(&base, &sample("aaa", "2026-06-13T00:00:00Z")).unwrap();
        save(&base, &sample("bbb", "2026-06-13T01:00:00Z")).unwrap();

        let listed = list(&base);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].id, "bbb"); // newest first
        assert_eq!(listed[0].n_segments, 1);

        let loaded = load(&base, "aaa").unwrap();
        assert_eq!(loaded.title, "T aaa");

        delete(&base, "aaa").unwrap();
        assert_eq!(list(&base).len(), 1);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let base = std::env::temp_dir();
        assert!(load(&base, "../secret").is_err());
        assert!(delete(&base, "a/b").is_err());
    }
}
