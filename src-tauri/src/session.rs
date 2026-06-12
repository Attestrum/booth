// Session state: one session.json per episode (atomic writes), take stacks on
// disk under narration/booth/. Rust is stateless between commands — every op is
// load -> mutate -> save, so a crash can never lose more than the op in flight.
use crate::script::{self, ScriptUnit};
use crate::wav;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits: u16,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Take {
    pub file: String,
    pub duration_sec: f64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recovered: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Passage {
    pub unit_start: usize,
    pub unit_end: usize,
    pub takes: Vec<Take>,
    pub accepted: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub schema: u32,
    pub episode: String,
    pub source: String,
    pub format: Option<AudioFormat>,
    pub units: Vec<ScriptUnit>,
    pub passages: Vec<Passage>,
    pub cursor: usize,
    pub created_at: String,
    pub device: Option<String>,
    /// Absolute path of the document this session's script was imported from
    /// (inline-edit write-back target). Absent on sessions made before the
    /// project model and on sessions opened from pre-built units files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_file: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub episode_dir: String,
    pub episode: String,
    pub recorded: usize,
    pub total: usize,
    pub takes: usize,
}

pub fn booth_dir(episode_dir: &Path) -> PathBuf {
    episode_dir.join("narration/booth")
}

fn session_path(episode_dir: &Path) -> PathBuf {
    booth_dir(episode_dir).join("session.json")
}

pub fn takes_dir(episode_dir: &Path) -> PathBuf {
    booth_dir(episode_dir).join("takes")
}

fn discarded_dir(episode_dir: &Path) -> PathBuf {
    booth_dir(episode_dir).join("discarded")
}

pub fn load(episode_dir: &Path) -> Result<Session> {
    let raw = fs::read_to_string(session_path(episode_dir)).context("read session.json")?;
    Ok(serde_json::from_str(&raw)?)
}

/// Atomic save: temp file + rename, so a crash never corrupts session.json.
pub fn save(episode_dir: &Path, session: &Session) -> Result<()> {
    let path = session_path(episode_dir);
    fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(session)?)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}

/// Open an episode: resume an existing session (running WAV crash-recovery on its
/// takes) or build a fresh one via the parse ladder. Returns (session, fresh).
pub fn open(episode_dir: &Path, now_iso: String) -> Result<(Session, bool)> {
    if session_path(episode_dir).exists() {
        let mut s = load(episode_dir)?;
        recover_takes(episode_dir, &mut s)?;
        save(episode_dir, &s)?;
        return Ok((s, false));
    }
    let (units, source) = script::load_units(episode_dir)?;
    let passages = script::default_grouping(&units)
        .into_iter()
        .map(|(unit_start, unit_end)| Passage {
            unit_start,
            unit_end,
            takes: Vec::new(),
            accepted: false,
        })
        .collect();
    let episode = episode_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let s = Session {
        schema: 1,
        episode,
        source,
        format: None,
        units,
        passages,
        cursor: 0,
        created_at: now_iso,
        device: None,
        source_file: None,
    };
    save(episode_dir, &s)?;
    Ok((s, true))
}

/// Patch stale RIFF headers (crash mid-take) and fix duration metadata.
fn recover_takes(episode_dir: &Path, session: &mut Session) -> Result<()> {
    let dir = takes_dir(episode_dir);
    for p in &mut session.passages {
        for t in &mut p.takes {
            let path = dir.join(&t.file);
            if !path.exists() {
                continue;
            }
            if wav::recover_wav(&path).unwrap_or(false) {
                t.recovered = true;
                t.duration_sec = wav::wav_duration_secs(&path).unwrap_or(t.duration_sec);
            }
        }
    }
    Ok(())
}

/// Scan a project folder for resumable sessions: the folder itself, plus its
/// immediate subfolders (a project can be one script's folder or a folder of
/// episode folders).
pub fn scan(root: &Path) -> Vec<SessionSummary> {
    let mut out = Vec::new();
    let mut consider = |dir: &Path| {
        if !session_path(dir).exists() {
            return;
        }
        if let Ok(s) = load(dir) {
            out.push(SessionSummary {
                episode_dir: dir.to_string_lossy().into_owned(),
                episode: s.episode.clone(),
                recorded: s.passages.iter().filter(|p| !p.takes.is_empty()).count(),
                total: s.passages.len(),
                takes: s.passages.iter().map(|p| p.takes.len()).sum(),
            });
        }
    };
    consider(root);
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                consider(&dir);
            }
        }
    }
    out.sort_by(|a, b| a.episode.cmp(&b.episode));
    out
}

/// Folders at or under `root` (one level) that booth can open fresh: they
/// carry a parseable narration script but no session yet.
pub fn list_candidates(root: &Path) -> Vec<String> {
    let openable = |d: &Path| {
        !session_path(d).exists()
            && (d.join("narration/script-units.json").exists()
                || d.join("narration/chunks.json").exists())
    };
    let mut out = Vec::new();
    if openable(root) {
        out.push(root.to_string_lossy().into_owned());
    }
    if let Ok(entries) = fs::read_dir(root) {
        for e in entries.flatten() {
            let d = e.path();
            if d.is_dir() && openable(&d) {
                out.push(d.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

/// Filename for the next take of a passage: p013_t02.wav (1-based, padded).
pub fn next_take_path(episode_dir: &Path, session: &Session, passage: usize) -> Result<PathBuf> {
    let p = session
        .passages
        .get(passage)
        .ok_or_else(|| anyhow!("passage {passage} out of range"))?;
    let dir = takes_dir(episode_dir);
    fs::create_dir_all(&dir)?;
    // next take number = count of takes ever made (look at disk to avoid collisions
    // with discarded takes that were popped from the stack)
    let prefix = format!("p{:03}_t", passage + 1);
    let mut max_t = p.takes.len();
    for d in [&dir, &discarded_dir(episode_dir)] {
        if let Ok(entries) = fs::read_dir(d) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(rest) = name.strip_prefix(&prefix) {
                    if let Ok(n) = rest.trim_end_matches(".wav").parse::<usize>() {
                        max_t = max_t.max(n);
                    }
                }
            }
        }
    }
    Ok(dir.join(format!("{prefix}{:02}.wav", max_t + 1)))
}

/// Pop the top take: move its file to discarded/ (never delete).
pub fn discard_top(episode_dir: &Path, session: &mut Session, passage: usize) -> Result<Take> {
    let len = session
        .passages
        .get(passage)
        .ok_or_else(|| anyhow!("passage {passage} out of range"))?
        .takes
        .len();
    if len == 0 {
        bail!("no takes to revert");
    }
    discard_at(episode_dir, session, passage, len - 1)
}

/// Remove the take at `index` (0 = oldest): move its file to discarded/
/// (never delete). Removing the top take clears `accepted`; removing a
/// lower take leaves the accepted top take alone.
pub fn discard_at(
    episode_dir: &Path,
    session: &mut Session,
    passage: usize,
    index: usize,
) -> Result<Take> {
    let p = session
        .passages
        .get_mut(passage)
        .ok_or_else(|| anyhow!("passage {passage} out of range"))?;
    if index >= p.takes.len() {
        bail!("take {index} out of range ({} takes)", p.takes.len());
    }
    let was_top = index == p.takes.len() - 1;
    let take = p.takes.remove(index);
    if was_top {
        p.accepted = false;
    }
    let from = takes_dir(episode_dir).join(&take.file);
    let to_dir = discarded_dir(episode_dir);
    fs::create_dir_all(&to_dir)?;
    if from.exists() {
        fs::rename(&from, to_dir.join(&take.file))?;
    }
    save(episode_dir, session)?;
    Ok(take)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end over a synthetic script-units.json: 40 units across 3
    /// chapters, sized so grouping must both merge units and break at
    /// chapter boundaries.
    #[test]
    fn open_builds_sane_passages() {
        let tmp = std::env::temp_dir().join("booth-session-test/ep999-test");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("narration")).unwrap();
        let units: Vec<serde_json::Value> = (0..40)
            .map(|i| {
                serde_json::json!({
                    "text": format!(
                        "Sentence number {i} carries enough words to make grouping \
                         meaningful without being long enough to stand alone."
                    ),
                    "cue": if i % 4 == 0 { format!("[VISUAL: beat {i}]") } else { String::new() },
                    "chapter": format!("CHAPTER {}", i / 14),
                })
            })
            .collect();
        fs::write(
            tmp.join("narration/script-units.json"),
            serde_json::to_string_pretty(&units).unwrap(),
        )
        .unwrap();

        let (s, fresh) = open(&tmp, "2026-06-12T00:00:00Z".into()).unwrap();
        assert!(fresh);
        assert_eq!(s.units.len(), 40);
        // merged below TARGET_CHARS but split across the 3 chapters: strictly
        // more passages than chapters, far fewer than units
        assert!(
            s.passages.len() >= 4 && s.passages.len() < 40,
            "got {} passages",
            s.passages.len()
        );
        // contiguous full coverage
        assert_eq!(s.passages[0].unit_start, 0);
        assert_eq!(s.passages.last().unwrap().unit_end, s.units.len() - 1);
        for w in s.passages.windows(2) {
            assert_eq!(w[0].unit_end + 1, w[1].unit_start);
        }
        // chapter barriers hold: no passage spans two chapters
        for p in &s.passages {
            let ch = &s.units[p.unit_start].chapter;
            assert!(
                (p.unit_start..=p.unit_end).all(|i| &s.units[i].chapter == ch),
                "passage spans a chapter boundary"
            );
        }
        // reopen resumes instead of rebuilding
        let (s2, fresh2) = open(&tmp, "2026-06-12T00:00:01Z".into()).unwrap();
        assert!(!fresh2);
        assert_eq!(s2.passages.len(), s.passages.len());
    }

    #[test]
    fn discard_at_mid_stack_and_top() {
        let tmp = std::env::temp_dir().join("booth-session-test/ep998-discard");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(booth_dir(&tmp)).unwrap();
        let take = |name: &str| Take {
            file: name.into(),
            duration_sec: 1.0,
            recovered: false,
        };
        let mut s = Session {
            schema: 1,
            episode: "ep998".into(),
            source: "test".into(),
            format: None,
            units: vec![],
            passages: vec![Passage {
                unit_start: 0,
                unit_end: 0,
                takes: vec![take("a.wav"), take("b.wav"), take("c.wav")],
                accepted: true,
            }],
            cursor: 0,
            created_at: "2026-06-12T00:00:00Z".into(),
            device: None,
            source_file: None,
        };
        // mid-stack delete: order preserved, accepted top take untouched
        let t = discard_at(&tmp, &mut s, 0, 1).unwrap();
        assert_eq!(t.file, "b.wav");
        assert!(s.passages[0].accepted);
        let files: Vec<_> = s.passages[0].takes.iter().map(|t| t.file.as_str()).collect();
        assert_eq!(files, ["a.wav", "c.wav"]);
        // top delete clears accepted (same semantics as revert)
        let t = discard_at(&tmp, &mut s, 0, 1).unwrap();
        assert_eq!(t.file, "c.wav");
        assert!(!s.passages[0].accepted);
        // error paths: bad index, bad passage, empty stack via discard_top
        assert!(discard_at(&tmp, &mut s, 0, 5).is_err());
        assert!(discard_at(&tmp, &mut s, 9, 0).is_err());
        discard_at(&tmp, &mut s, 0, 0).unwrap();
        assert!(discard_top(&tmp, &mut s, 0).is_err());
    }

    #[test]
    fn import_document_end_to_end() {
        let root = std::env::temp_dir().join("booth-session-test/import-root");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let md = root.join("my-script.md");
        fs::write(
            &md,
            "# Intro\n\nHello there. This is a script.\n\n[VISUAL: a beat]\n\n# Part Two\n\nMore words here.\n",
        )
        .unwrap();

        let (dir, s, fresh) = import_document(&md, "2026-06-12T00:00:00Z".into()).unwrap();
        assert!(fresh);
        assert_eq!(dir, root);
        assert_eq!(s.episode, "my-script", "session is named after the document");
        assert_eq!(s.source_file.as_deref(), Some(md.to_str().unwrap()));
        assert_eq!(s.units.len(), 3);
        assert_eq!(s.units[0].chapter, "Intro");
        assert_eq!(s.units[1].cue, "[VISUAL: a beat]");
        assert_eq!(s.units[2].chapter, "Part Two");
        assert!(root.join("narration/script-units.json").exists());

        // the imported folder is now scannable + resumable
        let found = scan(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].episode_dir, root.to_string_lossy());

        // re-import over an existing session is refused
        assert!(import_document(&md, "2026-06-12T00:00:01Z".into()).is_err());
        // unsupported extension is refused
        let pdf = root.join("nope.pdf");
        fs::write(&pdf, "x").unwrap();
        let err = match import_document(&pdf, "now".into()) {
            Err(e) => e.to_string(),
            Ok(_) => panic!("pdf import should be refused"),
        };
        assert!(err.contains(".md or .txt"), "got: {err}");
    }

    #[test]
    fn edit_unit_text_propagates_to_units_file_and_md() {
        let root = std::env::temp_dir().join("booth-session-test/edit-root");
        let _ = fs::remove_dir_all(&root);
        let ep = root.join("my-video");
        fs::create_dir_all(ep.join("narration")).unwrap();
        fs::write(
            ep.join("narration/script-units.json"),
            r#"[{"text":"The first sentence.","cue":"[VISUAL: a]","chapter":"CH1"},
                {"text":"The second sentence.","cue":"[VISUAL: b]","chapter":"CH1"}]"#,
        )
        .unwrap();
        let doc = root.join("my-video/script.md");
        fs::write(&doc, "# Script\n\nThe first sentence. The second sentence.\n").unwrap();

        let (mut s, _) = open(&ep, "2026-06-12T00:00:00Z".into()).unwrap();
        s.source_file = Some(doc.to_string_lossy().into_owned());
        let warns =
            edit_unit_text(&ep, &mut s, 1, "A rewritten second sentence.".into()).unwrap();
        assert!(warns.is_empty(), "no warnings expected, got {warns:?}");
        assert_eq!(s.units[1].text, "A rewritten second sentence.");

        // parse source updated (cue survives), session.json persisted with the link
        let raw = fs::read_to_string(ep.join("narration/script-units.json")).unwrap();
        assert!(raw.contains("A rewritten second sentence."));
        assert!(raw.contains("[VISUAL: b]"));
        let reloaded = load(&ep).unwrap();
        assert_eq!(reloaded.units[1].text, "A rewritten second sentence.");
        assert_eq!(reloaded.source_file.as_deref(), Some(doc.to_str().unwrap()));

        // linked source document updated by exact replace
        let md = fs::read_to_string(&doc).unwrap();
        assert!(md.contains("The first sentence. A rewritten second sentence."));

        // error paths: bad index, empty text; unchanged text is a silent no-op
        assert!(edit_unit_text(&ep, &mut s, 9, "x".into()).is_err());
        assert!(edit_unit_text(&ep, &mut s, 0, "  ".into()).is_err());
        assert!(edit_unit_text(&ep, &mut s, 0, "The first sentence.".into())
            .unwrap()
            .is_empty());
    }
}

/// Import a .md/.txt document: parse to units, persist them as the folder's
/// narration/script-units.json (the standard parse ladder takes over), open
/// the fresh session, and link `source_file` for inline-edit write-back.
/// Refuses folders that already have a session and non-md/txt extensions.
pub fn import_document(src: &Path, now_iso: String) -> Result<(PathBuf, Session, bool)> {
    let ext = src
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let markdown = matches!(ext.as_str(), "md" | "markdown");
    if !markdown && ext != "txt" {
        bail!(
            "can't import .{ext} — bring a .md or .txt script (PDF/docx: export to one of those first)"
        );
    }
    let dir = src
        .parent()
        .ok_or_else(|| anyhow!("script file has no parent folder"))?
        .to_path_buf();
    if session_path(&dir).exists() {
        bail!(
            "this folder already has a booth session — open it from the list instead \
             (or remove its narration/booth/ to re-import)"
        );
    }
    let raw = fs::read_to_string(src).with_context(|| format!("read {}", src.display()))?;
    let units = script::units_from_document(&raw, markdown);
    if units.is_empty() {
        bail!("no readable text found in that file");
    }
    fs::create_dir_all(dir.join("narration"))?;
    fs::write(
        dir.join("narration/script-units.json"),
        serde_json::to_string_pretty(&units)?,
    )?;
    let (mut session, fresh) = open(&dir, now_iso)?;
    session.source_file = Some(src.to_string_lossy().into_owned());
    // name the session after the DOCUMENT, not its folder — importing from
    // ~/Downloads must not produce a session called "Downloads"
    if let Some(stem) = src.file_stem() {
        session.episode = stem.to_string_lossy().into_owned();
    }
    save(&dir, &session)?;
    Ok((dir, session, fresh))
}

/// Edit one unit's text in place and propagate to the on-disk sources: the
/// units file this session was parsed from (narration/script-units.json or
/// chunks.json) and the session's linked `source_file` document (exact-match
/// replace, first occurrence). Returns warnings for propagation targets it
/// could not update; the session itself always updates.
pub fn edit_unit_text(
    episode_dir: &Path,
    session: &mut Session,
    unit: usize,
    new_text: String,
) -> Result<Vec<String>> {
    let new_text = new_text.trim().to_string();
    if new_text.is_empty() {
        bail!("unit text cannot be empty");
    }
    let old = session
        .units
        .get(unit)
        .ok_or_else(|| anyhow!("unit {unit} out of range"))?
        .text
        .clone();
    if old == new_text {
        return Ok(vec![]);
    }
    session.units[unit].text = new_text.clone();
    save(episode_dir, session)?;

    let mut warnings = Vec::new();

    // 1. the parse source — a future fresh session must see the edit
    let units_path = episode_dir.join("narration").join(&session.source);
    match fs::read_to_string(&units_path)
        .map_err(anyhow::Error::from)
        .and_then(|raw| Ok(serde_json::from_str::<serde_json::Value>(&raw)?))
    {
        Ok(mut json) => {
            let hit = json.as_array_mut().and_then(|arr| {
                arr.iter_mut()
                    .find(|u| u.get("text").and_then(|t| t.as_str()) == Some(old.as_str()))
            });
            match hit {
                Some(entry) => {
                    entry["text"] = serde_json::Value::String(new_text.clone());
                    if let Err(e) = fs::write(&units_path, serde_json::to_string_pretty(&json)?) {
                        warnings.push(format!("{}: {e}", session.source));
                    }
                }
                None => warnings.push(format!("old text not found in {}", session.source)),
            }
        }
        Err(e) => warnings.push(format!("{}: {e:#}", session.source)),
    }

    // 2. the linked source document (set at import / migration)
    match session.source_file.as_deref().map(PathBuf::from) {
        Some(doc) if doc.exists() => match fs::read_to_string(&doc) {
            Ok(raw) if raw.contains(&old) => {
                if let Err(e) = fs::write(&doc, raw.replacen(&old, &new_text, 1)) {
                    warnings.push(format!("source file: {e}"));
                }
            }
            Ok(_) => warnings.push("old text not found in the source file".into()),
            Err(e) => warnings.push(format!("source file: {e}")),
        },
        Some(doc) => warnings.push(format!("source file missing: {}", doc.display())),
        None => { /* no linked document — units file update above is the full propagation */ }
    }

    Ok(warnings)
}

/// Undo a discard: move the file back and re-push the take.
pub fn undo_discard(
    episode_dir: &Path,
    session: &mut Session,
    passage: usize,
    take: Take,
) -> Result<()> {
    let from = discarded_dir(episode_dir).join(&take.file);
    if !from.exists() {
        bail!("discarded take {} no longer on disk", take.file);
    }
    fs::rename(&from, takes_dir(episode_dir).join(&take.file))?;
    session
        .passages
        .get_mut(passage)
        .ok_or_else(|| anyhow!("passage {passage} out of range"))?
        .takes
        .push(take);
    save(episode_dir, session)?;
    Ok(())
}
