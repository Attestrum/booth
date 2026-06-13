//! Core data model for the transcription feature: a unified `Segment` shape
//! produced identically by caption import and Whisper, the saved `Transcript`
//! record (one JSON file per transcript in app-data/transcripts/), and the
//! timestamp formatting helpers shared by every exporter.

use serde::{Deserialize, Serialize};

/// Where a transcript's segments came from. One transcript = one source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentSource {
    ManualSubs,
    AutoSubs,
    Whisper,
}

impl SegmentSource {
    /// Stable lowercase label used in the UI and metadata.
    pub fn label(self) -> &'static str {
        match self {
            SegmentSource::ManualSubs => "manual-subs",
            SegmentSource::AutoSubs => "auto-subs",
            SegmentSource::Whisper => "whisper",
        }
    }
}

/// Whether the source was a remote URL or a local file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Url,
    File,
}

/// One timestamped line. Captions and Whisper both produce exactly this shape,
/// so the library JSON and every exporter consume a single unified type.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// A saved transcript — one file in `app-data/transcripts/<id>.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub id: String,
    pub title: String,
    /// URL or filename the audio came from.
    pub source: String,
    pub source_kind: SourceKind,
    pub segment_source: SegmentSource,
    /// Whisper model id (absent for caption imports).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub created_at: String,
    pub duration_sec: f64,
    pub segments: Vec<Segment>,
}

/// `HH:MM:SS,mmm` — SubRip (SRT) timestamp.
pub fn ts_srt(ms: u64) -> String {
    ts(ms, ',')
}

/// `HH:MM:SS.mmm` — WebVTT timestamp.
pub fn ts_vtt(ms: u64) -> String {
    ts(ms, '.')
}

/// `HH:MM:SS` — display / TXT / CSV timestamp (no milliseconds).
pub fn ts_hms(ms: u64) -> String {
    let (h, m, s, _) = split(ms);
    format!("{h:02}:{m:02}:{s:02}")
}

fn ts(ms: u64, sep: char) -> String {
    let (h, m, s, milli) = split(ms);
    format!("{h:02}:{m:02}:{s:02}{sep}{milli:03}")
}

fn split(ms: u64) -> (u64, u64, u64, u64) {
    (
        ms / 3_600_000,
        (ms % 3_600_000) / 60_000,
        (ms % 60_000) / 1000,
        ms % 1000,
    )
}

/// A readable paragraph: a run of segments merged into flowing prose, stamped
/// with the start time of its first segment.
pub struct Paragraph {
    pub start_ms: u64,
    pub text: String,
}

/// Group caption-sized segments into readable paragraphs (used by the result
/// view and the prose exporters — TXT/HTML/DOCX/PDF). Segments are joined with
/// spaces; a new paragraph starts once the current one is long enough AND ends
/// on sentence punctuation, or on a clear pause after a shorter sentence.
pub fn paragraphs(segments: &[Segment]) -> Vec<Paragraph> {
    const TARGET: usize = 500; // soft max chars before breaking at a sentence
    const MIN_PAUSE_BREAK: usize = 240; // min chars to break on a pause
    const PAUSE_MS: u64 = 2000;

    let mut out: Vec<Paragraph> = Vec::new();
    let mut cur = String::new();
    let mut start = 0u64;
    let mut last_end = 0u64;

    for seg in segments {
        let piece = seg.text.replace('\n', " ");
        let piece = piece.split_whitespace().collect::<Vec<_>>().join(" ");
        if piece.is_empty() {
            continue;
        }
        if cur.is_empty() {
            start = seg.start_ms;
        } else {
            let ends_sentence = cur.trim_end().ends_with(['.', '?', '!', '…']);
            let gap = seg.start_ms.saturating_sub(last_end);
            let long_enough = cur.len() >= TARGET;
            let paused = gap >= PAUSE_MS && cur.len() >= MIN_PAUSE_BREAK;
            if ends_sentence && (long_enough || paused) {
                out.push(Paragraph {
                    start_ms: start,
                    text: std::mem::take(&mut cur),
                });
                start = seg.start_ms;
            }
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&piece);
        last_end = seg.end_ms;
    }
    if !cur.is_empty() {
        out.push(Paragraph {
            start_ms: start,
            text: cur,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_format_each_flavor() {
        // 1h 02m 03s 004ms
        let ms = 3_723_004;
        assert_eq!(ts_srt(ms), "01:02:03,004");
        assert_eq!(ts_vtt(ms), "01:02:03.004");
        assert_eq!(ts_hms(ms), "01:02:03");
        assert_eq!(ts_srt(0), "00:00:00,000");
    }

    #[test]
    fn paragraphs_merge_segments_into_prose() {
        let seg = |a: u64, b: u64, t: &str| Segment {
            start_ms: a,
            end_ms: b,
            text: t.into(),
        };
        // short caption-style lines that form one sentence, then a pause + new one
        let segs = vec![
            seg(0, 1000, "90% of"),
            seg(1000, 2000, "the stuff headed to space"),
            seg(2000, 3000, "is carried on a rocket."),
            // long pause → new paragraph (prev ends with '.', but < MIN_PAUSE_BREAK
            // so it stays merged here; verify space-join + single paragraph)
            seg(3200, 4000, "Earth orbit is crowded."),
        ];
        let paras = paragraphs(&segs);
        assert_eq!(paras.len(), 1, "short content stays one paragraph");
        assert_eq!(
            paras[0].text,
            "90% of the stuff headed to space is carried on a rocket. Earth orbit is crowded."
        );
        assert_eq!(paras[0].start_ms, 0);
        // newlines inside a segment are flattened to spaces
        let p2 = paragraphs(&[seg(0, 1, "line one\nline two")]);
        assert_eq!(p2[0].text, "line one line two");
    }

    #[test]
    fn source_labels_are_stable() {
        assert_eq!(SegmentSource::ManualSubs.label(), "manual-subs");
        assert_eq!(SegmentSource::AutoSubs.label(), "auto-subs");
        assert_eq!(SegmentSource::Whisper.label(), "whisper");
    }

    #[test]
    fn transcript_round_trips_camel_case_json() {
        let t = Transcript {
            id: "abc".into(),
            title: "Demo".into(),
            source: "https://example.com/v".into(),
            source_kind: SourceKind::Url,
            segment_source: SegmentSource::AutoSubs,
            model: None,
            language: Some("en".into()),
            created_at: "2026-06-13T00:00:00Z".into(),
            duration_sec: 12.5,
            segments: vec![Segment {
                start_ms: 0,
                end_ms: 1000,
                text: "hi".into(),
            }],
        };
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains("\"segmentSource\":\"auto-subs\""));
        assert!(json.contains("\"sourceKind\":\"url\""));
        assert!(!json.contains("\"model\"")); // None is skipped
        let back: Transcript = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }
}
