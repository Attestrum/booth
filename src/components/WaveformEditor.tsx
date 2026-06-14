import { useEffect, useMemo, useRef, useState } from "react";
import { takeWaveform } from "../lib/ipc";
import { normalizeCuts, spansFromCuts, type Cut, type Take } from "../lib/session";

// Audacity-style ripple editor for the selected take. The strip shows the
// EDITED timeline: cut audio is removed and the waveform closes the gap, with a
// thin BREAK STUB marking each splice (click a stub to restore that cut).
//
//   • single click  → place the editing cursor (Play starts here)
//   • click + drag   → make a SELECTION (cyan); drag its edges to fine-tune
//   • Delete / ✄ CUT → ripple-delete the selection (gap closes, stub appears)
//   • click a stub   → restore that cut · RESTORE → revert the whole take
//   • during Play     → a bright playhead line sweeps the edited timeline
//
// Cuts are stored as Take.cuts in ORIGINAL seconds (the WAV is never touched);
// the display maps original↔kept time so the strip reflects the final audio.

const BUCKETS = 600;
const MIN_CUT = 0.04;
const CLICK_PX = 3;

const peakCache = new Map<string, number[]>();
const fmtSec = (s: number) => `${s.toFixed(2)}s`;

// original-time second → position along the collapsed (kept) timeline
const origToKept = (spans: [number, number][], sec: number): number => {
  let acc = 0;
  for (const [s, e] of spans) {
    if (sec < s) return acc; // inside a cut → snaps to the splice
    if (sec <= e) return acc + (sec - s);
    acc += e - s;
  }
  return acc;
};
// position along the collapsed timeline → original-time second
const keptToOrig = (spans: [number, number][], p: number): number => {
  let acc = 0;
  for (const [s, e] of spans) {
    const len = e - s;
    if (p <= acc + len) return s + (p - acc);
    acc += len;
  }
  return spans.length ? spans[spans.length - 1][1] : 0;
};

type Sel = { a: number; b: number }; // original seconds, unordered while dragging
const lo = (s: Sel) => Math.min(s.a, s.b);
const hi = (s: Sel) => Math.max(s.a, s.b);
type Drag = { mode: "create" | "left" | "right"; downX: number; moved: boolean };

export function WaveformEditor({
  episodeDir,
  take,
  takeFile,
  onCuts,
  onPlayTarget,
  playheadRef,
  playing,
  height = 110,
}: {
  episodeDir: string;
  take: Take;
  takeFile: string;
  onCuts: (cuts: Cut[]) => void;
  onPlayTarget: (start: number, end: number | null) => void;
  playheadRef: React.MutableRefObject<number>;
  playing: boolean;
  height?: number;
}) {
  const dur = take.durationSec || 0;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  const headRef = useRef<HTMLDivElement>(null);
  const [peaks, setPeaks] = useState<number[]>(() => peakCache.get(takeFile) ?? []);
  const [cuts, setCuts] = useState<Cut[]>(() => take.cuts ?? []);
  const [sel, setSel] = useState<Sel | null>(null);
  const [cursor, setCursor] = useState(0);
  const drag = useRef<Drag | null>(null);
  const selRef = useRef<Sel | null>(null);
  selRef.current = sel;
  const cursorRef = useRef(0);
  cursorRef.current = cursor;

  // the collapsed (kept) timeline
  const spans = useMemo(() => spansFromCuts(cuts, dur), [cuts, dur]);
  const totalKept = useMemo(
    () => spans.reduce((s, [a, b]) => s + (b - a), 0) || dur || 1,
    [spans, dur],
  );
  const pctK = (secOrig: number) => (origToKept(spans, secOrig) / totalKept) * 100;
  const secAt = (clientX: number) => {
    const r = boxRef.current!.getBoundingClientRect();
    const frac = Math.min(1, Math.max(0, (clientX - r.left) / r.width));
    return keptToOrig(spans, frac * totalKept);
  };

  useEffect(() => {
    const cached = peakCache.get(takeFile);
    if (cached) return setPeaks(cached);
    let alive = true;
    void takeWaveform(episodeDir, takeFile, BUCKETS).then((p) => {
      peakCache.set(takeFile, p);
      if (alive) setPeaks(p);
    });
    return () => {
      alive = false;
    };
  }, [episodeDir, takeFile]);

  useEffect(() => {
    setCuts(take.cuts ?? []);
    setSel(null);
    setCursor(0);
    onPlayTarget(0, null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [takeFile]);

  // draw the collapsed waveform: buckets inside a cut are dropped; kept buckets
  // fill the width in order, so the gap visibly closes
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d")!;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (canvas.width !== w * dpr || canvas.height !== h * dpr) {
      canvas.width = w * dpr;
      canvas.height = h * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const mid = h / 2;
    const n = peaks.length;
    if (n === 0 || dur === 0) return;
    const inCut = (sec: number) => cuts.some((c) => sec > c.startSec && sec < c.endSec);
    const keptBuckets = Math.max(1, Math.round((totalKept / dur) * n));
    ctx.strokeStyle = "#7FE0FF";
    ctx.lineWidth = Math.max(1, w / keptBuckets - 0.4);
    for (let i = 0; i < n; i++) {
      const t = (i / n) * dur;
      if (inCut(t)) continue;
      const x = (origToKept(spans, t) / totalKept) * w;
      const amp = Math.max(1, peaks[i] * (h * 0.46));
      ctx.beginPath();
      ctx.moveTo(x, mid - amp);
      ctx.lineTo(x, mid + amp);
      ctx.stroke();
    }
  }, [peaks, cuts, spans, totalKept, dur, height]);

  // moving playhead, mapped into collapsed time, nudged by rAF
  useEffect(() => {
    if (!playing) {
      if (headRef.current) headRef.current.style.display = "none";
      return;
    }
    let raf = 0;
    const tick = () => {
      const el = headRef.current;
      const t = playheadRef.current;
      if (el) {
        if (t >= 0) {
          el.style.display = "block";
          el.style.left = `${pctK(t)}%`;
        } else el.style.display = "none";
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, spans, totalKept]);

  const cutSelection = () => {
    const s = selRef.current;
    if (!s) return;
    const c = { startSec: lo(s), endSec: hi(s) };
    if (c.endSec - c.startSec < MIN_CUT) return;
    const next = normalizeCuts([...cuts, c], dur);
    setCuts(next);
    onCuts(next);
    setSel(null);
    onPlayTarget(cursor, null);
  };

  const restoreCut = (index: number) => {
    const next = cuts.filter((_, i) => i !== index);
    setCuts(next);
    onCuts(next);
  };
  const restoreAll = () => {
    if (cuts.length === 0) return;
    setCuts([]);
    onCuts([]);
  };

  useEffect(() => {
    const move = (e: PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      if (!d.moved && Math.abs(e.clientX - d.downX) < CLICK_PX) return;
      d.moved = true;
      const sec = secAt(e.clientX);
      if (d.mode === "left") setSel((s) => (s ? { ...s, a: sec } : s));
      else if (d.mode === "right") setSel((s) => (s ? { ...s, b: sec } : s));
      else setSel((s) => (s ? { ...s, b: sec } : { a: sec, b: sec }));
    };
    const up = () => {
      const d = drag.current;
      if (!d) return;
      drag.current = null;
      if (!d.moved && d.mode === "create") return; // a click → cursor only
      const s = selRef.current;
      if (s && hi(s) - lo(s) >= MIN_CUT) onPlayTarget(lo(s), hi(s));
      else {
        setSel(null);
        onPlayTarget(cursor, null);
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cursor, spans, totalKept]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "TEXTAREA" || t.tagName === "INPUT" || t.isContentEditable))
        return;
      if (!selRef.current) return;
      e.preventDefault();
      cutSelection();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cuts, cursor, dur]);

  // Click anywhere that isn't a button or the waveform strip to drop the
  // selection. Clicks inside the strip are left to onBgPointerDown (which
  // collapses the selection to a cursor); clicks on a button are preserved so
  // CUT / PLAY / REVERT can still act on the current selection.
  useEffect(() => {
    const onDocDown = (e: PointerEvent) => {
      if (!selRef.current) return;
      const el = e.target as Element | null;
      if (!el) return;
      if (boxRef.current?.contains(el)) return; // inside the strip → its own handler
      if (el.closest("button")) return; // a button → keep the selection
      setSel(null);
      onPlayTarget(cursorRef.current, null);
    };
    document.addEventListener("pointerdown", onDocDown);
    return () => document.removeEventListener("pointerdown", onDocDown);
  }, [onPlayTarget]);

  const onBgPointerDown = (e: React.PointerEvent) => {
    if (drag.current) return;
    const sec = secAt(e.clientX);
    setCursor(sec);
    setSel({ a: sec, b: sec });
    onPlayTarget(sec, null);
    drag.current = { mode: "create", downX: e.clientX, moved: false };
  };

  const keptSec = totalKept;
  const hasSel = sel != null && hi(sel) - lo(sel) >= 0.001;
  // sorted cuts so stub index lines up with the displayed splice order
  const sortedCuts = useMemo(
    () => cuts.map((c, i) => ({ c, i })).sort((a, b) => a.c.startSec - b.c.startSec),
    [cuts],
  );

  return (
    <div>
      <div
        ref={boxRef}
        onPointerDown={onBgPointerDown}
        style={{
          position: "relative",
          height,
          touchAction: "none",
          userSelect: "none",
          cursor: "text",
        }}
      >
        <canvas ref={canvasRef} style={{ width: "100%", height, display: "block" }} />

        {/* break stubs — one per cut, at the collapsed splice; click to restore */}
        {sortedCuts.map(({ c, i }) => (
          <button
            key={`stub-${i}`}
            type="button"
            title="Restore this cut"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={() => restoreCut(i)}
            style={{ ...stubBtn, left: `${pctK(c.startSec)}%` }}
          >
            <span style={stubLine} />
            <span style={stubCap}>▾</span>
          </button>
        ))}

        {/* live selection — clicks on the body fall through to the strip handler
            (which collapses the selection); only the edge handles capture, to resize */}
        {hasSel && sel && (
          <div
            style={{
              position: "absolute",
              top: 0,
              bottom: 0,
              left: `${pctK(lo(sel))}%`,
              width: `${pctK(hi(sel)) - pctK(lo(sel))}%`,
              background: "rgba(127,224,255,0.18)",
              borderLeft: "1px solid var(--cyan)",
              borderRight: "1px solid var(--cyan)",
              zIndex: 3,
            }}
          >
            <div
              onPointerDown={(e) => {
                e.stopPropagation();
                drag.current = { mode: "left", downX: e.clientX, moved: true };
              }}
              style={edgeHandle("left")}
            />
            <div
              onPointerDown={(e) => {
                e.stopPropagation();
                drag.current = { mode: "right", downX: e.clientX, moved: true };
              }}
              style={edgeHandle("right")}
            />
          </div>
        )}

        {/* editing cursor */}
        {!hasSel && (
          <div
            style={{
              position: "absolute",
              top: 0,
              bottom: 0,
              left: `${pctK(cursor)}%`,
              width: 1,
              background: "rgba(127,224,255,0.55)",
              zIndex: 3,
              pointerEvents: "none",
            }}
          />
        )}

        {/* moving playhead */}
        <div
          ref={headRef}
          style={{
            position: "absolute",
            top: 0,
            bottom: 0,
            left: 0,
            width: 2,
            marginLeft: -1,
            background: "var(--green)",
            boxShadow: "0 0 8px var(--green)",
            display: "none",
            zIndex: 4,
            pointerEvents: "none",
          }}
        />
      </div>

      <div style={readoutRow}>
        {hasSel && sel ? (
          <>
            <span>SEL {fmtSec(lo(sel))} → {fmtSec(hi(sel))}</span>
            <span style={{ color: "var(--cyan)" }}>LEN {fmtSec(hi(sel) - lo(sel))}</span>
            <button type="button" onClick={cutSelection} style={cutBtn}>
              ✄ CUT (DEL)
            </button>
          </>
        ) : (
          <span>CURSOR {fmtSec(cursor)}</span>
        )}
        <span>KEPT {fmtSec(keptSec)}</span>
        <span>
          {cuts.length} CUT{cuts.length === 1 ? "" : "S"}
        </span>
        {cuts.length > 0 && (
          <button
            type="button"
            onClick={restoreAll}
            title="Revert this take to its original, uncut state"
            style={restoreAllBtn}
          >
            ↩ RESTORE
          </button>
        )}
        <span style={{ flex: 1 }} />
        <span style={{ color: "var(--dim-cyan-soft)" }}>
          CLICK = CURSOR · DRAG = SELECT · DEL = CUT · ▾ = RESTORE THAT BREAK
        </span>
      </div>
    </div>
  );
}

const edgeHandle = (side: "left" | "right"): React.CSSProperties => ({
  position: "absolute",
  top: 0,
  bottom: 0,
  [side]: -4,
  width: 8,
  cursor: "ew-resize",
  zIndex: 4,
});

const stubBtn: React.CSSProperties = {
  position: "absolute",
  top: 0,
  bottom: 0,
  width: 12,
  marginLeft: -6,
  padding: 0,
  background: "transparent",
  border: "none",
  cursor: "pointer",
  zIndex: 3,
  display: "flex",
  justifyContent: "center",
};

const stubLine: React.CSSProperties = {
  position: "absolute",
  top: 0,
  bottom: 0,
  width: 2,
  marginLeft: -1,
  left: "50%",
  background: "var(--amber)",
  boxShadow: "0 0 6px rgba(255,176,0,0.6)",
};

const stubCap: React.CSSProperties = {
  position: "absolute",
  top: -2,
  fontSize: 9,
  lineHeight: "9px",
  color: "var(--amber)",
};

const readoutRow: React.CSSProperties = {
  display: "flex",
  gap: 16,
  alignItems: "center",
  fontSize: 10,
  letterSpacing: "0.15em",
  color: "var(--dim-cyan)",
  marginTop: 2,
};

const cutBtn: React.CSSProperties = {
  fontSize: 10,
  letterSpacing: "0.15em",
  color: "rgba(255,150,150,0.95)",
  background: "rgba(20,0,0,0.5)",
  border: "1px solid rgba(255,120,120,0.6)",
  borderRadius: 3,
  padding: "2px 8px",
  cursor: "pointer",
};

const restoreAllBtn: React.CSSProperties = {
  fontSize: 10,
  letterSpacing: "0.15em",
  color: "var(--cyan)",
  background: "rgba(0,20,30,0.5)",
  border: "1px solid var(--dim-cyan)",
  borderRadius: 3,
  padding: "2px 8px",
  cursor: "pointer",
};
