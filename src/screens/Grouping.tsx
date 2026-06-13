import { useMemo, useRef, useState, useEffect } from "react";
import { saveSession } from "../lib/ipc";
import { playSfx } from "../lib/sfx";
import { useKeymap } from "../hooks/useKeymap";
import { Btn } from "../components/Btn";
import type { Session, Passage } from "../lib/session";
import { passageText, passageChapter } from "../lib/session";

// Grouping editor — J/K move, M merge with next, S split at midpoint unit,
// Enter confirm. Passages with takes are locked (merge/split would orphan audio).
export function Grouping({
  episodeDir,
  session,
  onSession,
  onConfirm,
  onBack,
}: {
  episodeDir: string;
  session: Session;
  onSession: (s: Session) => void;
  onConfirm: () => void;
  onBack: () => void;
}) {
  const [sel, setSel] = useState(session.cursor);
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listRef.current
      ?.children[sel]?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [sel]);

  const chapters = useMemo(() => {
    // passage index -> chapter label when it starts a new chapter
    const labels = new Map<number, string>();
    let prev = "";
    session.passages.forEach((p, i) => {
      const ch = passageChapter(session, p);
      if (ch && ch !== prev) labels.set(i, ch);
      prev = ch;
    });
    return labels;
  }, [session]);

  // begin carries the SELECTED row into the booth — sel is local state, so
  // without this the booth resumed at the old session.cursor (gap fix #15)
  const begin = () => {
    playSfx("toggle");
    const s = { ...session, cursor: sel };
    onSession(s);
    void saveSession(episodeDir, s);
    onConfirm();
  };

  const update = (passages: Passage[], nextSel?: number) => {
    const s = { ...session, passages };
    onSession(s);
    void saveSession(episodeDir, s);
    if (nextSel !== undefined) setSel(nextSel);
  };

  const merge = () => {
    const a = session.passages[sel];
    const b = session.passages[sel + 1];
    if (!a || !b) return playSfx("error", 0.3);
    if (a.takes.length || b.takes.length) return playSfx("error", 0.3);
    const merged: Passage = {
      unitStart: a.unitStart,
      unitEnd: b.unitEnd,
      takes: [],
      accepted: false,
    };
    playSfx("toggle", 0.4);
    update([
      ...session.passages.slice(0, sel),
      merged,
      ...session.passages.slice(sel + 2),
    ]);
  };

  const split = () => {
    const p = session.passages[sel];
    if (!p || p.takes.length) return playSfx("error", 0.3);
    if (p.unitEnd === p.unitStart) return playSfx("error", 0.3);
    const mid = Math.floor((p.unitStart + p.unitEnd) / 2);
    playSfx("toggle", 0.4);
    update([
      ...session.passages.slice(0, sel),
      { unitStart: p.unitStart, unitEnd: mid, takes: [], accepted: false },
      { unitStart: mid + 1, unitEnd: p.unitEnd, takes: [], accepted: false },
      ...session.passages.slice(sel + 1),
    ]);
  };

  useKeymap(
    {
      j: () => {
        setSel((s) => Math.min(s + 1, session.passages.length - 1));
        playSfx("nav", 0.25);
      },
      arrowdown: () => {
        setSel((s) => Math.min(s + 1, session.passages.length - 1));
        playSfx("nav", 0.25);
      },
      k: () => {
        setSel((s) => Math.max(s - 1, 0));
        playSfx("nav", 0.25);
      },
      arrowup: () => {
        setSel((s) => Math.max(s - 1, 0));
        playSfx("nav", 0.25);
      },
      m: merge,
      s: split,
      enter: () => begin(),
      escape: () => {
        playSfx("nav", 0.3);
        onBack();
      },
    },
    [session, sel],
  );

  return (
    <div className="screen" style={{ padding: "72px 90px 64px" }}>
      <div
        style={{
          color: "var(--dim-cyan)",
          letterSpacing: "0.42em",
          fontSize: 13,
          marginBottom: 8,
        }}
      >
        {session.episode.toUpperCase()} ▸ TRANSCRIPT
      </div>
      <div
        style={{
          color: "var(--dim-cyan)",
          fontSize: 11,
          letterSpacing: "0.1em",
          marginBottom: 24,
        }}
      >
        {session.passages.length} passages from {session.units.length} units (
        {session.source})
      </div>

      <div ref={listRef} style={{ overflowY: "auto", flex: 1 }}>
        {session.passages.map((p, i) => {
          const active = i === sel;
          const locked = p.takes.length > 0;
          return (
            <div key={`${p.unitStart}-${p.unitEnd}`}>
              {chapters.has(i) && (
                <div
                  style={{
                    color: "var(--dim-cyan)",
                    fontSize: 10,
                    letterSpacing: "0.3em",
                    margin: "18px 0 8px",
                    borderBottom: "1px solid var(--faint-cyan)",
                    paddingBottom: 4,
                  }}
                >
                  {chapters.get(i)?.toUpperCase()}
                </div>
              )}
              <div
                onClick={() => setSel(i)}
                style={{
                  display: "flex",
                  gap: 16,
                  padding: "10px 14px",
                  marginBottom: 4,
                  border: `1px solid ${active ? "var(--dim-cyan)" : "transparent"}`,
                  background: active ? "var(--faint-cyan)" : "transparent",
                  cursor: "pointer",
                }}
              >
                <span
                  style={{
                    color: locked ? "var(--green)" : "var(--dim-cyan)",
                    fontSize: 11,
                    width: 44,
                    flexShrink: 0,
                  }}
                >
                  {String(i + 1).padStart(3, "0")}
                  {locked ? " ●" : ""}
                </span>
                <span
                  style={{
                    // no faded text (gap #28): inactive rows use the soft tier,
                    // not opacity stacked on dim.
                    color: active ? "var(--cyan)" : "var(--dim-cyan-soft)",
                    fontSize: 13,
                    lineHeight: 1.5,
                  }}
                >
                  {passageText(session, p)}
                </span>
              </div>
            </div>
          );
        })}
      </div>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          marginTop: 20,
        }}
      >
        <Btn
          id="episodes"
          label="‹ Episodes"
          hint="esc"
          onClick={() => {
            playSfx("nav", 0.3);
            onBack();
          }}
        />
        <span style={{ flex: 1 }} />
        <Btn
          id="merge"
          label="Merge ▾"
          hint="m"
          disabled={
            !session.passages[sel + 1] ||
            session.passages[sel].takes.length > 0 ||
            session.passages[sel + 1].takes.length > 0
          }
          onClick={merge}
        />
        <Btn
          id="split"
          label="Split ▸▸"
          hint="s"
          disabled={
            session.passages[sel].takes.length > 0 ||
            session.passages[sel].unitEnd === session.passages[sel].unitStart
          }
          onClick={split}
        />
        <span style={{ width: 18 }} />
        <Btn
          id="begin"
          label="Begin ▸ Booth"
          hint="⏎"
          variant="success"
          onClick={begin}
        />
      </div>
      <div
        style={{
          color: "var(--dim-cyan)",
          fontSize: 10,
          letterSpacing: "0.22em",
          marginTop: 10,
        }}
      >
        J/K NAVIGATE · ● = HAS TAKES (LOCKED FROM MERGE/SPLIT)
      </div>
    </div>
  );
}
