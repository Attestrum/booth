// Script ingestion: parse ladder (script-units.json -> chunks.json -> raw text)
// + default passage grouping (greedy merge within chapter, ~450-char target).
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
pub struct ScriptUnit {
    pub text: String,
    #[serde(default)]
    pub cue: String,
    #[serde(default)]
    pub chapter: String,
}

pub const TARGET_CHARS: usize = 600; // ~35-40 s spoken at 150 wpm

/// Load units for an episode folder via the parse ladder.
/// Returns (units, source-name).
pub fn load_units(episode_dir: &Path) -> Result<(Vec<ScriptUnit>, String)> {
    let units_path = episode_dir.join("narration/script-units.json");
    if units_path.exists() {
        let units: Vec<ScriptUnit> =
            serde_json::from_str(&fs::read_to_string(&units_path)?)
                .context("parse script-units.json")?;
        if !units.is_empty() {
            return Ok((units, "script-units.json".into()));
        }
    }
    let chunks_path = episode_dir.join("narration/chunks.json");
    if chunks_path.exists() {
        let chunks: Vec<String> = serde_json::from_str(&fs::read_to_string(&chunks_path)?)
            .context("parse chunks.json")?;
        let units = chunks
            .iter()
            .flat_map(|c| split_sentences(c))
            .map(|text| ScriptUnit {
                text,
                cue: String::new(),
                chapter: String::new(),
            })
            .collect::<Vec<_>>();
        if !units.is_empty() {
            return Ok((units, "chunks.json".into()));
        }
    }
    bail!("no narration/script-units.json or narration/chunks.json in this folder")
}

/// Units from pasted raw text: blank-line paragraphs, sentence-split.
#[allow(dead_code)] // paste-raw-text fallback, wired when the paste UI lands
pub fn units_from_text(text: &str) -> Vec<ScriptUnit> {
    text.split("\n\n")
        .flat_map(|p| split_sentences(p))
        .map(|text| ScriptUnit {
            text,
            cue: String::new(),
            chapter: String::new(),
        })
        .collect()
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
        if word.ends_with(['.', '!', '?', '…']) && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Greedy default grouping: merge consecutive units within one chapter until
/// the passage would exceed TARGET_CHARS. Returns (start, end) inclusive pairs.
pub fn default_grouping(units: &[ScriptUnit]) -> Vec<(usize, usize)> {
    let mut passages = Vec::new();
    let mut start = 0usize;
    let mut chars = 0usize;
    for i in 0..units.len() {
        let len = units[i].text.len();
        let new_chapter = i > 0 && units[i].chapter != units[i - 1].chapter;
        if i > start && (new_chapter || chars + len > TARGET_CHARS) {
            passages.push((start, i - 1));
            start = i;
            chars = 0;
        }
        chars += len;
    }
    if start < units.len() {
        passages.push((start, units.len() - 1));
    }
    passages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_respects_chapters_and_target() {
        let units: Vec<ScriptUnit> = (0..10)
            .map(|i| ScriptUnit {
                text: "x".repeat(280),
                cue: String::new(),
                chapter: if i < 5 { "A".into() } else { "B".into() },
            })
            .collect();
        let g = default_grouping(&units);
        // 200-char units, 450 target -> pairs; chapter break at 5
        assert!(g.iter().all(|&(s, e)| units[s].chapter == units[e].chapter));
        assert_eq!(g.first().unwrap().0, 0);
        assert_eq!(g.last().unwrap().1, 9);
        // contiguous, no gaps
        for w in g.windows(2) {
            assert_eq!(w[0].1 + 1, w[1].0);
        }
    }

    #[test]
    fn sentence_split_basic() {
        let s = split_sentences("One two. Three! Four? Five");
        assert_eq!(s, vec!["One two.", "Three!", "Four?", "Five"]);
    }
}
