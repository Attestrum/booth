// TS mirror of the Rust session.json schema (src-tauri/src/session.rs).

export interface AudioFormat {
  sampleRate: number;
  channels: number;
  bits: number;
}

export interface ScriptUnit {
  text: string;
  cue: string;
  chapter: string;
}

export interface Take {
  file: string; // filename inside booth/takes/
  durationSec: number;
  recovered?: boolean;
}

export interface Passage {
  unitStart: number; // inclusive index into units
  unitEnd: number; // inclusive
  takes: Take[]; // stack: last = top = kept take
  accepted: boolean;
}

export interface Session {
  schema: 1;
  episode: string; // episode folder name
  source: string; // which file the units came from
  format: AudioFormat | null; // latest take's format (display only)
  units: ScriptUnit[];
  passages: Passage[];
  cursor: number; // current passage index
  createdAt: string;
  device: string | null;
  sourceFile?: string | null; // imported document — inline-edit write-back target
}

export const passageText = (s: Session, p: Passage): string =>
  s.units
    .slice(p.unitStart, p.unitEnd + 1)
    .map((u) => u.text)
    .join(" ");

export const passageChapter = (s: Session, p: Passage): string =>
  s.units[p.unitStart]?.chapter ?? "";

export const topTake = (p: Passage): Take | undefined =>
  p.takes[p.takes.length - 1];
