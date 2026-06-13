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
