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

/// Units from an imported markdown / plain-text document.
///
/// Rules (conservative — the transcript screen is the repair tool):
/// - blank-line paragraphs, each sentence-split into units
/// - markdown only: `#`–`######` headings set the `chapter` for everything
///   until the next heading; fenced ``` code blocks are skipped; light inline
///   strip (emphasis markers, backticks, `[text](url)` → text, list/quote
///   markers)
/// - a paragraph that starts with `[VISUAL:` or `[CUE:` becomes the cue of
///   the PRECEDING unit (works in .md and .txt — keeps cue-annotated scripts
///   on the same import path)
pub fn units_from_document(raw: &str, markdown: bool) -> Vec<ScriptUnit> {
    let mut out: Vec<ScriptUnit> = Vec::new();
    let mut chapter = String::new();
    let mut para = String::new();
    let mut in_fence = false;

    let flush = |para: &mut String, chapter: &str, out: &mut Vec<ScriptUnit>| {
        let text = para.trim().to_string();
        para.clear();
        if text.is_empty() {
            return;
        }
        let upper = text.to_uppercase();
        if upper.starts_with("[VISUAL:") || upper.starts_with("[CUE:") {
            if let Some(last) = out.last_mut() {
                last.cue = text;
            }
            return;
        }
        out.extend(split_sentences(&text).into_iter().map(|t| ScriptUnit {
            text: t,
            cue: String::new(),
            chapter: chapter.to_string(),
        }));
    };

    for line in raw.replace("\r\n", "\n").lines() {
        let trimmed = line.trim();
        if markdown && trimmed.starts_with("```") {
            flush(&mut para, &chapter, &mut out);
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if markdown && trimmed.starts_with('#') {
            flush(&mut para, &chapter, &mut out);
            chapter = trimmed.trim_start_matches('#').trim().to_string();
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut para, &chapter, &mut out);
            continue;
        }
        let cleaned = if markdown {
            strip_md_inline(trimmed)
        } else {
            trimmed.to_string()
        };
        if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(&cleaned);
    }
    flush(&mut para, &chapter, &mut out);
    out
}

/// Light markdown inline strip: list/quote markers, emphasis, backticks,
/// `[text](url)` → text. Deliberately lossy-conservative — never drops words.
fn strip_md_inline(line: &str) -> String {
    let mut s = line;
    for marker in ["- ", "* ", "+ ", "> "] {
        if let Some(rest) = s.strip_prefix(marker) {
            s = rest;
            break;
        }
    }
    let mut text = s.replace("**", "").replace("__", "").replace('`', "");
    // [text](url) → text, repeated; ![alt](url) → alt
    loop {
        let Some(open) = text.find('[') else { break };
        let Some(mid) = text[open..].find("](").map(|i| open + i) else { break };
        let Some(close) = text[mid..].find(')').map(|i| mid + i) else { break };
        let label = text[open + 1..mid].to_string();
        let start = if open > 0 && text.as_bytes()[open - 1] == b'!' {
            open - 1
        } else {
            open
        };
        text.replace_range(start..=close, &label);
    }
    text
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

    #[test]
    fn markdown_import_chapters_cues_and_strip() {
        let md = "\
# Cold Open

Before the boom there was a **crawler** copying the [web](https://example.com).
It came from a tiny nonprofit.

[VISUAL: anim:RadarSweep — recon scan]

## The Archive

```
code blocks are skipped entirely
```

- A bulleted line still reads as `prose`.
";
        let units = units_from_document(md, true);
        let texts: Vec<&str> = units.iter().map(|u| u.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "Before the boom there was a crawler copying the web.",
                "It came from a tiny nonprofit.",
                "A bulleted line still reads as prose.",
            ]
        );
        assert_eq!(units[0].chapter, "Cold Open");
        assert_eq!(units[2].chapter, "The Archive");
        // the cue paragraph attached to the unit before it
        assert_eq!(units[1].cue, "[VISUAL: anim:RadarSweep — recon scan]");
        assert!(units[0].cue.is_empty());
    }

    #[test]
    fn txt_import_plain_paragraphs() {
        let txt = "First sentence. Second sentence!\n\nNew paragraph here.";
        let units = units_from_document(txt, false);
        let texts: Vec<&str> = units.iter().map(|u| u.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["First sentence.", "Second sentence!", "New paragraph here."]
        );
        assert!(units.iter().all(|u| u.chapter.is_empty()));
    }
}
