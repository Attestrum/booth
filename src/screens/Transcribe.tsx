import { useEffect, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  transcribe as startTranscribe,
  openTranscript,
  exportTranscript,
  onTranscribeProgress,
  onTranscribeDone,
  onTranscribeError,
} from "../lib/ipc";
import type { Transcript, TranscribeProgress } from "../lib/transcript";
import { EXPORT_FORMATS } from "../lib/transcript";
import { playSfx } from "../lib/sfx";
import { useKeymap } from "../hooks/useKeymap";
import { Btn } from "../components/Btn";
import { GlitchFlash } from "../components/GlitchFlash";

// Launched either to RUN a new job (url/file) or to OPEN a saved transcript.
export type TranscribeArg =
  | { mode: "run"; kind: "url" | "file"; value: string }
  | { mode: "open"; id: string };

function hms(ms: number) {
  const s = Math.floor(ms / 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(s / 3600))}:${p(Math.floor((s % 3600) / 60))}:${p(s % 60)}`;
}

function sanitize(title: string) {
  return title.replace(/[/\\:*?"<>|]+/g, " ").replace(/\s+/g, " ").trim() || "transcript";
}

export function Transcribe({
  arg,
  onBack,
}: {
  arg: TranscribeArg;
  onBack: () => void;
}) {
  const [log, setLog] = useState<TranscribeProgress[]>([]);
  const [transcript, setTranscript] = useState<Transcript | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sealFlash, setSealFlash] = useState(0);
  const [savedTo, setSavedTo] = useState<string | null>(null);
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;

    if (arg.mode === "open") {
      openTranscript(arg.id)
        .then(setTranscript)
        .catch((e) => setError(String(e)));
      return;
    }

    const unsubs = [
      onTranscribeProgress((p) => setLog((l) => [...l, p])),
      onTranscribeDone((t) => {
        setTranscript(t);
        playSfx("chime", 0.4);
      }),
      onTranscribeError((m) => {
        setError(m);
        playSfx("error");
      }),
    ];
    startTranscribe(arg.kind, arg.value).catch((e) => setError(String(e)));
    return () => unsubs.forEach((u) => u.then((f) => f()));
  }, [arg]);

  useKeymap({ escape: () => onBack() }, []);

  const doExport = async (fmt: string) => {
    if (!transcript) return;
    try {
      const dest = await saveDialog({
        title: `Export transcript (.${fmt})`,
        defaultPath: `${sanitize(transcript.title)}.${fmt}`,
      });
      if (typeof dest !== "string") return;
      await exportTranscript(transcript.id, fmt, dest);
      setSavedTo(dest);
      setSealFlash((n) => n + 1);
      playSfx("chime", 0.4);
    } catch (e) {
      playSfx("error");
      setError(String(e));
    }
  };

  const running = arg.mode === "run" && !transcript && !error;

  return (
    <div className="screen" style={{ padding: "72px 90px 64px" }}>
      <div
        style={{
          color: "var(--dim-cyan)",
          letterSpacing: "0.42em",
          fontSize: 13,
          marginBottom: 28,
        }}
      >
        ATTESTRUM // BOOTH ▸ TRANSCRIBE
        {transcript ? " ▸ RESULT" : ""}
      </div>

      {/* source line — wraps, never cropped */}
      <div
        style={{
          color: "var(--dim-cyan)",
          fontSize: 12,
          marginBottom: 18,
          wordBreak: "break-all",
          lineHeight: 1.6,
        }}
      >
        SOURCE&nbsp;&nbsp;
        {transcript
          ? transcript.source
          : arg.mode === "run"
            ? arg.value
            : ""}
        {transcript && (
          <>
            {"  ·  "}
            <span style={{ color: "var(--cyan)", letterSpacing: "0.1em" }}>
              {transcript.segmentSource.toUpperCase()}
            </span>
            {transcript.model ? `  ·  ${transcript.model}` : ""}
            {transcript.durationSec
              ? `  ·  ${hms(transcript.durationSec * 1000)}`
              : ""}
          </>
        )}
      </div>

      {/* RUNNING — staged progress log */}
      {running && (
        <div className="tx-log">
          {log.length === 0 && <div style={{ color: "var(--dim-cyan)" }}>starting…</div>}
          {log.map((p, i) => {
            const last = i === log.length - 1;
            return (
              <div
                key={i}
                style={{
                  color: last ? "var(--cyan)" : "var(--dim-cyan)",
                  lineHeight: 1.8,
                  display: "flex",
                  alignItems: "center",
                  gap: 14,
                }}
              >
                <span>{p.stage}</span>
                {p.pct != null && <Bar pct={p.pct} />}
                {p.pct != null && (
                  <span style={{ fontSize: 11 }}>{p.pct}%</span>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* RESULT — segments */}
      {transcript && (
        <div style={{ overflowY: "auto", flex: 1, marginBottom: 16 }}>
          {transcript.segments.map((s, i) => (
            <div
              key={i}
              style={{ display: "flex", gap: 18, margin: "5px 0", fontSize: 13 }}
            >
              <span
                style={{
                  color: "var(--dim-cyan)",
                  flex: "0 0 84px",
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {hms(s.startMs)}
              </span>
              <span style={{ color: "var(--cyan)", whiteSpace: "pre-wrap", flex: 1 }}>
                {s.text}
              </span>
            </div>
          ))}
        </div>
      )}

      {error && (
        <div style={{ color: "var(--amber)", fontSize: 12, marginTop: 16, lineHeight: 1.6 }}>
          ⚠ {error}
        </div>
      )}

      {savedTo && (
        <div style={{ marginTop: 10 }}>
          <GlitchFlash fire={sealFlash}>
            <span
              style={{
                color: "var(--green)",
                letterSpacing: "0.2em",
                fontSize: 12,
                textShadow: "0 0 12px rgba(127,255,176,0.5)",
              }}
            >
              ✓ SAVED ▸ {savedTo}
            </span>
          </GlitchFlash>
        </div>
      )}

      {/* bottom bar */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          marginTop: 20,
          gap: 8,
          flexWrap: "wrap",
        }}
      >
        <span
          style={{ color: "var(--dim-cyan)", fontSize: 10, letterSpacing: "0.25em" }}
        >
          ESC ▸ BACK
        </span>
        <span style={{ flex: 1 }} />
        {transcript && (
          <>
            <span
              style={{
                color: "var(--dim-cyan)",
                fontSize: 10,
                letterSpacing: "0.2em",
                marginRight: 4,
              }}
            >
              EXPORT ▸
            </span>
            {EXPORT_FORMATS.map((f) => (
              <Btn
                key={f}
                id={`export-${f}`}
                label={f}
                onClick={() => void doExport(f)}
              />
            ))}
          </>
        )}
      </div>
    </div>
  );
}

function Bar({ pct }: { pct: number }) {
  return (
    <div
      style={{
        width: 180,
        height: 4,
        background: "var(--faint-cyan)",
        position: "relative",
      }}
    >
      <div
        style={{
          position: "absolute",
          inset: 0,
          width: `${Math.max(0, Math.min(100, pct))}%`,
          background: "var(--cyan)",
          boxShadow: "0 0 8px var(--dim-cyan)",
          transition: "width 160ms ease",
        }}
      />
    </div>
  );
}
