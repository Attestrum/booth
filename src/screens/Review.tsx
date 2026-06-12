import { useEffect, useRef, useState } from "react";
import { exportSession, onExportProgress } from "../lib/ipc";
import { playSfx } from "../lib/sfx";
import { useKeymap } from "../hooks/useKeymap";
import { GlitchFlash } from "../components/GlitchFlash";
import { Btn } from "../components/Btn";
import type { Session } from "../lib/session";
import { passageText, topTake } from "../lib/session";

const fmt = (s: number) =>
  `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

type ExportState =
  | { phase: "idle" }
  | { phase: "running"; log: string[] }
  | { phase: "sealed"; log: string[]; wav: string; mp3: string }
  | { phase: "error"; log: string[]; message: string };

// Screen 3 — REVIEW ledger + export. J/K move, Enter/click jumps to that
// passage in the Booth. ⌘E export, ⇧⌘E partial, TAB/ESC back.
export function Review({
  episodeDir,
  session,
  onBack,
  onJump,
}: {
  episodeDir: string;
  session: Session;
  onBack: () => void;
  onJump: (passage: number) => void;
}) {
  const [exp, setExp] = useState<ExportState>({ phase: "idle" });
  const [sealFlash, setSealFlash] = useState(0);
  const [sel, setSel] = useState(session.cursor);
  const listRef = useRef<HTMLDivElement>(null);
  const expRef = useRef(exp);
  expRef.current = exp;

  useEffect(() => {
    listRef.current
      ?.children[sel]?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [sel]);

  useEffect(() => {
    const un = onExportProgress((msg) => {
      setExp((e) =>
        e.phase === "running" ? { ...e, log: [...e.log, msg] } : e,
      );
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const recorded = session.passages.filter((p) => p.takes.length > 0);
  const missing = session.passages.length - recorded.length;
  const runtime =
    recorded.reduce((s, p) => s + (topTake(p)?.durationSec ?? 0), 0) +
    Math.max(0, session.passages.length - 1) * 0.35;

  const runExport = async (allowPartial: boolean) => {
    if (expRef.current.phase === "running") return;
    if (missing > 0 && !allowPartial) {
      playSfx("error", 0.4);
      setExp({
        phase: "error",
        log: [],
        message: `${missing} passages unrecorded — EXPORT PARTIAL to override`,
      });
      return;
    }
    setExp({ phase: "running", log: ["EXPORT ▸ BEGIN"] });
    try {
      const { wav, mp3 } = await exportSession(episodeDir, allowPartial);
      playSfx("chime", 0.6);
      setSealFlash((f) => f + 1);
      setExp((e) => ({
        phase: "sealed",
        log: e.phase === "running" ? e.log : [],
        wav,
        mp3,
      }));
    } catch (e) {
      playSfx("error", 0.4);
      setExp((s) => ({
        phase: "error",
        log: s.phase === "running" ? s.log : [],
        message: String(e),
      }));
    }
  };

  useKeymap(
    {
      tab: onBack,
      escape: onBack,
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
      enter: () => onJump(sel),
      "cmd+e": () => void runExport(false),
      "cmd+shift+e": () => void runExport(true),
    },
    [session, missing, sel],
  );

  return (
    <div className="screen" style={{ padding: "56px 70px 28px" }}>
      <div
        style={{
          color: "var(--dim-cyan)",
          letterSpacing: "0.42em",
          fontSize: 13,
          marginBottom: 20,
        }}
      >
        {session.episode.toUpperCase()} ▸ REVIEW
      </div>

      <div ref={listRef} style={{ overflowY: "auto", flex: 1 }}>
        {session.passages.map((p, i) => {
          const t = topTake(p);
          const active = i === sel;
          return (
            <div
              key={`${p.unitStart}-${p.unitEnd}`}
              className="load-row"
              data-autopilot={`review-row-${i}`}
              onClick={() => onJump(i)}
              style={{
                display: "flex",
                gap: 16,
                padding: "7px 10px",
                fontSize: 12,
                color: t ? "var(--dim-cyan)" : "var(--faint-cyan)",
                border: `1px solid ${active ? "var(--dim-cyan)" : "transparent"}`,
                background: active ? "var(--faint-cyan)" : "transparent",
                borderBottom: active
                  ? "1px solid var(--dim-cyan)"
                  : "1px solid rgba(127,224,255,0.06)",
              }}
            >
              <span style={{ width: 36 }}>{String(i + 1).padStart(3, "0")}</span>
              <span
                style={{
                  width: 16,
                  color: t ? "var(--green)" : "var(--amber)",
                }}
              >
                {t ? "✓" : "◌"}
              </span>
              <span
                style={{
                  flex: 1,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {passageText(session, p)}
              </span>
              <span style={{ width: 60, textAlign: "right" }}>
                {p.takes.length > 0 ? `${p.takes.length}t` : "—"}
              </span>
              <span style={{ width: 60, textAlign: "right" }}>
                {t ? fmt(t.durationSec) : ""}
              </span>
            </div>
          );
        })}
      </div>

      {/* footer totals */}
      <div
        style={{
          display: "flex",
          gap: 32,
          marginTop: 16,
          color: "var(--dim-cyan)",
          fontSize: 12,
          letterSpacing: "0.15em",
        }}
      >
        <span>
          {recorded.length}/{session.passages.length} RECORDED
        </span>
        <span>{recorded.reduce((s, p) => s + p.takes.length, 0)} TAKES</span>
        <span>≈ {fmt(runtime)} RUNTIME</span>
      </div>

      {/* export status */}
      {exp.phase !== "idle" && (
        <div
          style={{
            marginTop: 14,
            padding: "14px 18px",
            border: `1px solid ${
              exp.phase === "sealed"
                ? "var(--green)"
                : exp.phase === "error"
                  ? "var(--amber)"
                  : "var(--dim-cyan)"
            }`,
            fontSize: 12,
          }}
        >
          {exp.phase !== "error" &&
            exp.log.map((l, i) => (
              <div key={i} style={{ color: "var(--dim-cyan)", lineHeight: 1.7 }}>
                {l}
              </div>
            ))}
          {exp.phase === "sealed" && (
            <>
              <div style={{ margin: "8px 0" }}>
                <GlitchFlash fire={sealFlash}>
                  <span
                    style={{
                      color: "var(--green)",
                      letterSpacing: "0.3em",
                      textShadow: "0 0 12px rgba(127,255,176,0.5)",
                    }}
                  >
                    ✓ TRANSMISSION SEALED
                  </span>
                </GlitchFlash>
              </div>
              <div style={{ color: "var(--dim-cyan)" }}>{exp.wav}</div>
              <div style={{ color: "var(--dim-cyan)" }}>{exp.mp3}</div>
            </>
          )}
          {exp.phase === "error" && (
            <div style={{ color: "var(--amber)" }}>⚠ {exp.message}</div>
          )}
        </div>
      )}

      {/* footer controls */}
      <div
        style={{ display: "flex", alignItems: "center", gap: 10, marginTop: 16 }}
      >
        <Btn id="booth" label="‹ Booth" hint="⇥/esc" onClick={onBack} />
        <span
          style={{
            color: "var(--faint-cyan)",
            fontSize: 10,
            letterSpacing: "0.22em",
            marginLeft: 8,
          }}
        >
          J/K · ENTER / CLICK ROW ▸ JUMP TO PASSAGE
        </span>
        <span style={{ flex: 1 }} />
        {missing > 0 && (
          <Btn
            id="export-partial"
            label={`Export partial (${missing} missing)`}
            hint="⇧⌘E"
            variant="danger"
            onClick={() => void runExport(true)}
          />
        )}
        <Btn
          id="export"
          label="Export"
          hint="⌘E"
          variant="success"
          disabled={missing > 0}
          onClick={() => void runExport(false)}
        />
      </div>
    </div>
  );
}
