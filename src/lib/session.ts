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

export interface Cut {
  startSec: number;
  endSec: number;
}

export interface Take {
  file: string; // filename inside booth/takes/
  durationSec: number;
  recovered?: boolean;
  cuts?: Cut[]; // non-destructive seconds-ranges to REMOVE (head/tail/interior)
}

export interface Passage {
  unitStart: number; // inclusive index into units
  unitEnd: number; // inclusive
  takes: Take[]; // newest last; the SELECTED take is the kept one (see selectedTake)
  accepted: boolean;
  selected?: number; // index of the kept take; unset/out-of-range = newest
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

// The kept take: the explicitly-selected one if valid, else the newest (last).
// Single source of truth for play / accept / export — mirrors Rust selected_take.
export const selectedIndex = (p: Passage): number =>
  p.selected != null && p.selected >= 0 && p.selected < p.takes.length
    ? p.selected
    : p.takes.length - 1;

export const selectedTake = (p: Passage): Take | undefined =>
  p.takes[selectedIndex(p)];

// Clamp to [0,dur], drop empties, sort, merge overlapping/adjacent cuts.
// Mirrors Rust Take::kept_spans normalization so play/export/UI agree.
export const normalizeCuts = (cuts: Cut[], dur: number): Cut[] => {
  const cleaned = cuts
    .map((c) => ({
      startSec: Math.max(0, Math.min(c.startSec, dur)),
      endSec: Math.max(0, Math.min(c.endSec, dur)),
    }))
    .filter((c) => c.endSec - c.startSec > 0.02)
    .sort((a, b) => a.startSec - b.startSec);
  const merged: Cut[] = [];
  for (const c of cleaned) {
    const last = merged[merged.length - 1];
    if (last && c.startSec <= last.endSec + 0.01)
      last.endSec = Math.max(last.endSec, c.endSec);
    else merged.push({ ...c });
  }
  return merged;
};

// Spans (seconds) to KEEP: [0,dur] minus the cuts. Shared by playback, export,
// and the editor's collapsed (ripple) timeline.
export const spansFromCuts = (cuts: Cut[], dur: number): [number, number][] => {
  const norm = normalizeCuts(cuts, dur);
  const spans: [number, number][] = [];
  let pos = 0;
  for (const c of norm) {
    if (c.startSec > pos) spans.push([pos, c.startSec]);
    pos = Math.max(pos, c.endSec);
  }
  if (pos < dur) spans.push([pos, dur]);
  return spans;
};

export const keptSpans = (t: Take): [number, number][] =>
  spansFromCuts(t.cuts ?? [], t.durationSec || 0);

// Total kept (audible) length of a take after cuts.
export const keptDuration = (t: Take): number =>
  keptSpans(t).reduce((s, [a, b]) => s + (b - a), 0);

// Intersect kept spans with a [from,to] play window (cursor or selection).
export const clipSpans = (
  spans: [number, number][],
  from: number,
  to: number,
): [number, number][] =>
  spans
    .map(([a, b]) => [Math.max(a, from), Math.min(b, to)] as [number, number])
    .filter(([a, b]) => b - a > 0.001);
