// TS mirror of the Rust transcript model (src-tauri/src/transcript.rs +
// transcripts.rs). camelCase matches the serde rename.

export type SegmentSource = "manual-subs" | "auto-subs" | "whisper";
export type SourceKind = "url" | "file";

export interface Segment {
  startMs: number;
  endMs: number;
  text: string;
}

export interface Transcript {
  id: string;
  title: string;
  source: string;
  sourceKind: SourceKind;
  segmentSource: SegmentSource;
  model?: string | null;
  language?: string | null;
  createdAt: string;
  durationSec: number;
  segments: Segment[];
}

export interface TranscriptSummary {
  id: string;
  title: string;
  source: string;
  sourceKind: SourceKind;
  segmentSource: SegmentSource;
  model?: string | null;
  createdAt: string;
  durationSec: number;
  nSegments: number;
}

export interface TranscribeProgress {
  stage: string;
  pct: number | null;
}

// The 8 export formats, in display order.
export const EXPORT_FORMATS = [
  "txt",
  "srt",
  "vtt",
  "json",
  "csv",
  "html",
  "docx",
  "pdf",
] as const;
export type ExportFormat = (typeof EXPORT_FORMATS)[number];

export interface FormatInfo {
  ext: ExportFormat;
  label: string;
  desc: string;
}

// Shown in the export chooser; order matches the macOS-style two-column grid.
export const FORMAT_INFO: FormatInfo[] = [
  { ext: "txt", label: "Plain text (.txt)", desc: "Readable transcript in paragraphs, no timestamps. Best for emailing, pasting into Notes / Word." },
  { ext: "srt", label: "SubRip (.srt)", desc: "SubRip captions for video subtitles. Plays in YouTube, VLC, Premiere, Final Cut, DaVinci." },
  { ext: "vtt", label: "WebVTT (.vtt)", desc: "WebVTT captions for HTML5 <video> on the web. Same use as .srt, standard for browser playback." },
  { ext: "html", label: "HTML (.html)", desc: "Standalone web page with timestamps. Opens in any browser. Good for sharing a readable transcript." },
  { ext: "docx", label: "Microsoft Word (.docx)", desc: "Word document with formatting. Use when you need to edit further in Word, Pages, or Google Docs." },
  { ext: "pdf", label: "PDF (.pdf)", desc: "PDF for archival, sharing, or printing. Read-only output that looks the same on any device." },
  { ext: "json", label: "JSON (.json)", desc: "Whisper-shaped JSON. Drop into jq pipelines, LLM prompts, WhisperX, or any tool that speaks Whisper." },
  { ext: "csv", label: "CSV (.csv)", desc: "Spreadsheet-friendly. UTF-8 with BOM for Excel-on-Mac. One row per segment with start, end, text." },
];

export interface Paragraph {
  startMs: number;
  text: string;
}

// Group caption-sized segments into readable paragraphs (mirrors the Rust
// transcript::paragraphs used by the prose exporters, so the on-screen view and
// the exported TXT/HTML/DOCX/PDF read the same).
export function paragraphs(segments: Segment[]): Paragraph[] {
  const TARGET = 500;
  const MIN_PAUSE_BREAK = 240;
  const PAUSE_MS = 2000;

  const out: Paragraph[] = [];
  let cur = "";
  let start = 0;
  let lastEnd = 0;

  for (const seg of segments) {
    const piece = seg.text.replace(/\s+/g, " ").trim();
    if (!piece) continue;
    if (cur === "") {
      start = seg.startMs;
    } else {
      const endsSentence = /[.?!…]$/.test(cur.replace(/\s+$/, ""));
      const gap = seg.startMs - lastEnd;
      if (
        endsSentence &&
        (cur.length >= TARGET || (gap >= PAUSE_MS && cur.length >= MIN_PAUSE_BREAK))
      ) {
        out.push({ startMs: start, text: cur });
        cur = "";
        start = seg.startMs;
      }
    }
    cur = cur ? `${cur} ${piece}` : piece;
    lastEnd = seg.endMs;
  }
  if (cur) out.push({ startMs: start, text: cur });
  return out;
}
