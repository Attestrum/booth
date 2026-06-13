//! Render a [`Transcript`] to any of the eight export formats. Six are plain
//! string-building (no external crate); DOCX uses `docx-rs`; PDF uses `genpdfi`
//! (a maintained genpdf fork) with the bundled IBM Plex Mono family.

use crate::transcript::{ts_hms, ts_srt, ts_vtt, Transcript};
use anyhow::{Context, Result};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Txt,
    Srt,
    Vtt,
    Json,
    Csv,
    Html,
    Docx,
    Pdf,
}

impl Format {
    pub fn from_ext(ext: &str) -> Option<Format> {
        Some(match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "txt" => Format::Txt,
            "srt" => Format::Srt,
            "vtt" => Format::Vtt,
            "json" => Format::Json,
            "csv" => Format::Csv,
            "html" | "htm" => Format::Html,
            "docx" => Format::Docx,
            "pdf" => Format::Pdf,
            _ => return None,
        })
    }

    pub fn ext(self) -> &'static str {
        match self {
            Format::Txt => "txt",
            Format::Srt => "srt",
            Format::Vtt => "vtt",
            Format::Json => "json",
            Format::Csv => "csv",
            Format::Html => "html",
            Format::Docx => "docx",
            Format::Pdf => "pdf",
        }
    }
}

/// Render the transcript to bytes ready to write to disk. `font_dir` is only
/// consulted for PDF (the genpdf font family lives there); the other formats
/// ignore it.
pub fn render(t: &Transcript, fmt: Format, font_dir: &Path) -> Result<Vec<u8>> {
    Ok(match fmt {
        Format::Txt => txt(t).into_bytes(),
        Format::Srt => srt(t).into_bytes(),
        Format::Vtt => vtt(t).into_bytes(),
        Format::Json => json(t)?.into_bytes(),
        Format::Csv => csv(t).into_bytes(),
        Format::Html => html(t).into_bytes(),
        Format::Docx => docx(t)?,
        Format::Pdf => pdf(t, font_dir)?,
    })
}

// ---- plain-text family --------------------------------------------------

fn txt(t: &Transcript) -> String {
    t.segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn srt(t: &Transcript) -> String {
    let mut out = String::new();
    for (i, s) in t.segments.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            ts_srt(s.start_ms),
            ts_srt(s.end_ms),
            s.text
        ));
    }
    out
}

fn vtt(t: &Transcript) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for s in &t.segments {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            ts_vtt(s.start_ms),
            ts_vtt(s.end_ms),
            s.text
        ));
    }
    out
}

fn json(t: &Transcript) -> Result<String> {
    let full_text = t
        .segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    // Whisper-shaped: a flat `text` plus the segment array, alongside our meta.
    let v = serde_json::json!({
        "id": t.id,
        "title": t.title,
        "source": t.source,
        "sourceKind": t.source_kind,
        "segmentSource": t.segment_source,
        "model": t.model,
        "language": t.language,
        "createdAt": t.created_at,
        "durationSec": t.duration_sec,
        "text": full_text,
        "segments": t.segments,
    });
    Ok(serde_json::to_string_pretty(&v)?)
}

fn csv(t: &Transcript) -> String {
    // UTF-8 BOM so Excel-on-Mac reads it as UTF-8.
    let mut out = String::from("\u{feff}start_seconds,end_seconds,start_hms,end_hms,text\n");
    for s in &t.segments {
        out.push_str(&format!(
            "{:.3},{:.3},{},{},{}\n",
            s.start_ms as f64 / 1000.0,
            s.end_ms as f64 / 1000.0,
            ts_hms(s.start_ms),
            ts_hms(s.end_ms),
            csv_field(&s.text),
        ));
    }
    out
}

fn csv_field(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\"").replace('\n', " "))
}

fn html(t: &Transcript) -> String {
    let mut rows = String::new();
    for s in &t.segments {
        rows.push_str(&format!(
            "<div class=\"seg\"><span class=\"ts\">{}</span><span class=\"tx\">{}</span></div>\n",
            ts_hms(s.start_ms),
            esc(&s.text)
        ));
    }
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\n\
<title>{title}</title>\n<style>\n\
:root{{--cyan:#7fe0ff;--bg:#0a0e14;--dim:rgba(127,224,255,.72)}}\n\
body{{background:var(--bg);color:var(--cyan);font-family:'IBM Plex Mono',ui-monospace,monospace;\
max-width:820px;margin:40px auto;padding:0 20px;line-height:1.6}}\n\
h1{{font-size:18px;letter-spacing:.04em}}\n\
.meta{{color:var(--dim);font-size:12px;margin-bottom:24px}}\n\
.seg{{display:flex;gap:16px;margin:6px 0}}\n\
.ts{{color:var(--dim);flex:0 0 84px;font-variant-numeric:tabular-nums}}\n\
.tx{{white-space:pre-wrap}}\n</style></head>\n<body>\n\
<h1>{title}</h1>\n<div class=\"meta\">{src} · {ssrc}{model} · {dur}</div>\n{rows}</body></html>\n",
        title = esc(&t.title),
        src = esc(&t.source),
        ssrc = t.segment_source.label(),
        model = t
            .model
            .as_deref()
            .map(|m| format!(" · {}", esc(m)))
            .unwrap_or_default(),
        dur = ts_hms((t.duration_sec * 1000.0) as u64),
        rows = rows,
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ---- binary family ------------------------------------------------------

fn docx(t: &Transcript) -> Result<Vec<u8>> {
    use docx_rs::*;

    let mut doc = Docx::new().add_paragraph(
        Paragraph::new().add_run(Run::new().add_text(&t.title).bold().size(32)),
    );
    for s in &t.segments {
        doc = doc.add_paragraph(
            Paragraph::new()
                .add_run(Run::new().add_text(format!("[{}] ", ts_hms(s.start_ms))).bold())
                .add_run(Run::new().add_text(&s.text)),
        );
    }
    let mut cur = std::io::Cursor::new(Vec::new());
    doc.build()
        .pack(&mut cur)
        .map_err(|e| anyhow::anyhow!("docx pack: {e:?}"))?;
    Ok(cur.into_inner())
}

fn pdf(t: &Transcript, font_dir: &Path) -> Result<Vec<u8>> {
    use genpdfi::elements::{Break, Paragraph};
    use genpdfi::style::Style;
    use genpdfi::Element; // brings `.styled(..)` into scope

    let family = genpdfi::fonts::from_files(font_dir, "IBMPlexMono", None)
        .context("load PDF font family (IBM Plex Mono)")?;
    let mut doc = genpdfi::Document::new(family);
    doc.set_title(&t.title);
    doc.set_minimal_conformance();
    let mut deco = genpdfi::SimplePageDecorator::new();
    deco.set_margins(15);
    doc.set_page_decorator(deco);

    doc.push(Paragraph::new(&t.title).styled(Style::new().bold().with_font_size(15)));
    doc.push(Paragraph::new(format!(
        "{} · {}{}",
        t.source,
        t.segment_source.label(),
        t.model.as_deref().map(|m| format!(" · {m}")).unwrap_or_default(),
    ))
    .styled(Style::new().with_font_size(8)));
    doc.push(Break::new(1.0));

    for s in &t.segments {
        let mut p = Paragraph::default();
        p.push_styled(format!("[{}]  ", ts_hms(s.start_ms)), Style::new().bold());
        p.push(&s.text);
        doc.push(p);
    }

    let mut buf = Vec::new();
    doc.render(&mut buf).context("render PDF")?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{Segment, SegmentSource, SourceKind};
    use std::path::PathBuf;

    fn sample() -> Transcript {
        Transcript {
            id: "t1".into(),
            title: "Sample".into(),
            source: "clip.mp4".into(),
            source_kind: SourceKind::File,
            segment_source: SegmentSource::Whisper,
            model: Some("large-v3-turbo".into()),
            language: Some("en".into()),
            created_at: "2026-06-13T00:00:00Z".into(),
            duration_sec: 5.0,
            segments: vec![
                Segment { start_ms: 0, end_ms: 2000, text: "first <line>".into() },
                Segment { start_ms: 2000, end_ms: 5000, text: "second \"quoted\"".into() },
            ],
        }
    }

    fn font_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/fonts")
    }

    #[test]
    fn srt_is_well_formed() {
        let out = String::from_utf8(render(&sample(), Format::Srt, &font_dir()).unwrap()).unwrap();
        assert!(out.starts_with("1\n00:00:00,000 --> 00:00:02,000\nfirst <line>\n\n"));
        assert!(out.contains("2\n00:00:02,000 --> 00:00:05,000\n"));
    }

    #[test]
    fn vtt_has_header_and_dot_timestamps() {
        let out = String::from_utf8(render(&sample(), Format::Vtt, &font_dir()).unwrap()).unwrap();
        assert!(out.starts_with("WEBVTT\n\n"));
        assert!(out.contains("00:00:00.000 --> 00:00:02.000"));
    }

    #[test]
    fn csv_has_bom_and_escapes_quotes() {
        let out = String::from_utf8(render(&sample(), Format::Csv, &font_dir()).unwrap()).unwrap();
        assert!(out.starts_with('\u{feff}'));
        assert!(out.contains("\"second \"\"quoted\"\"\""));
    }

    #[test]
    fn html_escapes_markup() {
        let out = String::from_utf8(render(&sample(), Format::Html, &font_dir()).unwrap()).unwrap();
        assert!(out.contains("first &lt;line&gt;"));
        assert!(out.contains("<title>Sample</title>"));
    }

    #[test]
    fn json_is_whisper_shaped() {
        let out = String::from_utf8(render(&sample(), Format::Json, &font_dir()).unwrap()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["segmentSource"], "whisper");
        assert_eq!(v["segments"].as_array().unwrap().len(), 2);
        assert!(v["text"].as_str().unwrap().contains("first"));
    }

    #[test]
    fn docx_is_a_zip() {
        let out = render(&sample(), Format::Docx, &font_dir()).unwrap();
        assert_eq!(&out[0..2], b"PK"); // zip local-file-header magic
        assert!(out.len() > 200);
    }

    #[test]
    fn pdf_has_header() {
        let out = render(&sample(), Format::Pdf, &font_dir()).unwrap();
        assert_eq!(&out[0..5], b"%PDF-");
        assert!(out.len() > 500);
    }
}
