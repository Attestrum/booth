//! Hand-rolled SRT + WebVTT parsing into the unified [`Segment`] model.
//!
//! Handles the real-world hazards the prior Swift parser learned about: a
//! leading UTF-8 BOM, mixed `\r\n` / `\r` / `\n` line endings, the `WEBVTT`
//! header and `NOTE` / `STYLE` / `REGION` blocks, optional cue identifiers,
//! cue-setting tokens after the end timestamp, and the inline `<…>` timing
//! tags YouTube auto-caption VTT is littered with. One parser covers both
//! formats because a cue is located by its `-->` line, not by position.

use crate::transcript::Segment;

/// Parse SRT or WebVTT text into segments. Unrecognized blocks are skipped
/// rather than erroring — partial captions still yield a usable transcript.
pub fn parse(content: &str) -> Vec<Segment> {
    let norm = content
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");

    let mut out = Vec::new();
    for block in norm.split("\n\n") {
        let block = block.trim_matches('\n');
        if block.trim().is_empty() {
            continue;
        }
        let head = block.trim_start();
        if head.starts_with("WEBVTT")
            || head.starts_with("NOTE")
            || head.starts_with("STYLE")
            || head.starts_with("REGION")
        {
            continue;
        }

        let lines: Vec<&str> = block.lines().collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let Some((a, b)) = lines[ti].split_once("-->") else {
            continue;
        };
        let (Some(start_ms), Some(end_ms)) = (parse_ts(a), parse_ts(b)) else {
            continue;
        };

        let text = lines[ti + 1..]
            .iter()
            .map(|l| strip_tags(l))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
        if text.is_empty() {
            continue;
        }
        out.push(Segment {
            start_ms,
            end_ms,
            text,
        });
    }
    out
}

/// Parse `HH:MM:SS,mmm` / `HH:MM:SS.mmm` / `MM:SS.mmm`, tolerating a trailing
/// cue-settings tail (`... position:0% align:start`) by taking the first token.
fn parse_ts(s: &str) -> Option<u64> {
    let s = s.trim().split_whitespace().next()?;
    let (hms, milli) = if let Some((a, b)) = s.rsplit_once(',') {
        (a, b)
    } else if let Some((a, b)) = s.rsplit_once('.') {
        (a, b)
    } else {
        (s, "0")
    };
    let milli: u64 = milli.parse().ok()?;
    let mut it = hms.split(':').rev();
    let sec: u64 = it.next()?.parse().ok()?;
    let min: u64 = match it.next() {
        Some(x) => x.parse().ok()?,
        None => 0,
    };
    let hr: u64 = match it.next() {
        Some(x) => x.parse().ok()?,
        None => 0,
    };
    Some((hr * 3600 + min * 60 + sec) * 1000 + milli)
}

/// Drop `<…>` markup (VTT inline timing tags, `<i>`/`<b>` styling) so segment
/// text is plain. Subtitle text effectively never contains a literal `<`.
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0u32;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_srt_with_bom_and_crlf() {
        let srt = "\u{feff}1\r\n00:00:00,000 --> 00:00:02,500\r\nHello world\r\n\r\n\
                   2\r\n00:00:02,500 --> 00:00:05,000\r\nSecond line\r\n";
        let segs = parse(srt);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].start_ms, 0);
        assert_eq!(segs[0].end_ms, 2500);
        assert_eq!(segs[0].text, "Hello world");
        assert_eq!(segs[1].start_ms, 2500);
        assert_eq!(segs[1].text, "Second line");
    }

    #[test]
    fn parses_multiline_srt_cue() {
        let srt = "1\n00:00:00,000 --> 00:00:03,000\nline one\nline two\n";
        let segs = parse(srt);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "line one\nline two");
    }

    #[test]
    fn parses_vtt_header_settings_and_inline_tags() {
        let vtt = "WEBVTT\nKind: captions\nLanguage: en\n\n\
                   NOTE this is a note\n\n\
                   00:00:00.000 --> 00:00:02.000 align:start position:0%\n\
                   <00:00:00.320><c>Hello</c> <00:00:00.800><c>there</c>\n\n\
                   00:00:02.000 --> 00:00:04.000\nplain text\n";
        let segs = parse(vtt);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].start_ms, 0);
        assert_eq!(segs[0].end_ms, 2000);
        assert_eq!(segs[0].text, "Hello there");
        assert_eq!(segs[1].text, "plain text");
    }

    #[test]
    fn parses_vtt_short_timestamp_without_hours() {
        let vtt = "WEBVTT\n\n01:02.500 --> 01:05.000\nshort form\n";
        let segs = parse(vtt);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start_ms, 62_500); // 1m 02.5s
        assert_eq!(segs[0].end_ms, 65_000);
    }

    #[test]
    fn skips_garbage_blocks_without_panicking() {
        let junk = "not a subtitle\n\n\n\nWEBVTT\n\nbroken --> \n\n\
                    00:00:01,000 --> 00:00:02,000\nok\n";
        let segs = parse(junk);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "ok");
    }
}
