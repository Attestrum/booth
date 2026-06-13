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
import type {
  Transcript,
  TranscribeProgress,
  ExportFormat,
} from "../lib/transcript";
import { FORMAT_INFO, paragraphs } from "../lib/transcript";
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
  const [tick, setTick] = useState(0);
  const [showExport, setShowExport] = useState(false);
  const [exportFmt, setExportFmt] = useState<ExportFormat>("txt");
  const started = useRef(false);

  const running = arg.mode === "run" && !transcript && !error;

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

  // tick an elapsed counter on the current stage so long extractions (yt-dlp
  // ~25s) don't read as frozen; reset it whenever a new stage arrives.
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, [running]);
  useEffect(() => setTick(0), [log.length]);

  useKeymap({ escape: () => onBack() }, []);

  const doExport = async (fmt: ExportFormat) => {
    if (!transcript) return;
    setShowExport(false);
    try {
      const dest = await saveDialog({
        title: "Export Transcript",
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
                {last && p.pct == null && tick > 0 && (
                  <span style={{ fontSize: 11, color: "var(--dim-cyan-soft)" }}>
                    {tick}s
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* RESULT — flowing paragraphs (timestamp marker above each) */}
      {transcript && (
        <div style={{ overflowY: "auto", flex: 1, marginBottom: 16, maxWidth: 900 }}>
          {paragraphs(transcript.segments).map((p, i) => (
            <div key={i} style={{ margin: "0 0 22px" }}>
              <div
                style={{
                  color: "var(--dim-cyan)",
                  fontSize: 11,
                  letterSpacing: "0.06em",
                  marginBottom: 6,
                  fontVariantNumeric: "tabular-nums",
                }}
              >
                {hms(p.startMs)}
              </div>
              <p
                style={{
                  color: "var(--cyan)",
                  fontSize: 14,
                  lineHeight: 1.75,
                  margin: 0,
                }}
              >
                {p.text}
              </p>
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
          <Btn
            id="export"
            label="Export…"
            variant="success"
            onClick={() => setShowExport(true)}
          />
        )}
      </div>

      {showExport && transcript && (
        <ExportSheet
          title={transcript.title}
          selected={exportFmt}
          onSelect={setExportFmt}
          onCancel={() => setShowExport(false)}
          onSave={() => void doExport(exportFmt)}
        />
      )}
    </div>
  );
}

// Format chooser — a single EXPORT opens this; pick a format, then Save opens
// the native location dialog. Mirrors the macOS export panel, booth-styled.
function ExportSheet({
  title,
  selected,
  onSelect,
  onCancel,
  onSave,
}: {
  title: string;
  selected: ExportFormat;
  onSelect: (f: ExportFormat) => void;
  onCancel: () => void;
  onSave: () => void;
}) {
  useKeymap({ escape: onCancel, enter: onSave }, [selected]);
  return (
    <div className="export-overlay" onClick={onCancel}>
      <div className="export-panel" onClick={(e) => e.stopPropagation()}>
        <div className="export-title">EXPORT TRANSCRIPT</div>
        <div className="export-sub">
          Choose a format for “{title}”. Save picks the location.
        </div>
        <div className="export-grid">
          {FORMAT_INFO.map((f) => {
            const on = f.ext === selected;
            return (
              <button
                key={f.ext}
                type="button"
                className={`export-opt${on ? " export-opt--on" : ""}`}
                data-autopilot={`fmt-${f.ext}`}
                onClick={() => onSelect(f.ext)}
                onDoubleClick={onSave}
              >
                <span className="export-radio">{on ? "◉" : "○"}</span>
                <span className="export-opt-body">
                  <span className="export-opt-label">{f.label}</span>
                  <span className="export-opt-desc">{f.desc}</span>
                </span>
              </button>
            );
          })}
        </div>
        <div className="export-actions">
          <Btn id="export-cancel" label="Cancel" onClick={onCancel} />
          <Btn id="export-save" label="Save…" variant="success" onClick={onSave} />
        </div>
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
