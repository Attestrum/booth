import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  currentDevice,
  discardTake,
  discardTakeAt,
  editUnitText,
  saveSession,
  startRecording,
  stopRecording,
  takePath,
  undoDiscard,
  type DeviceInfo,
} from "../lib/ipc";
import { playSfx, setRecordingGate } from "../lib/sfx";
import { useAudioFrames } from "../hooks/useAudioFrames";
import { useKeymap } from "../hooks/useKeymap";
import { Oscilloscope } from "../components/Oscilloscope";
import { LevelMeter } from "../components/LevelMeter";
import { TakeStack } from "../components/TakeStack";
import { Teleprompter } from "../components/Teleprompter";
import { GlitchFlash } from "../components/GlitchFlash";
import { Btn, RecBtn } from "../components/Btn";
import type { Session, Take } from "../lib/session";
import { passageChapter, topTake } from "../lib/session";

const fmtClock = (ms: number) => {
  const s = ms / 1000;
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(Math.floor(s % 60)).padStart(2, "0")}.${Math.floor((s % 1) * 10)}`;
};

// Screen 2 — the core loop. SPACE rec/stop · P play · R-R revert · U undo ·
// ENTER accept▸next · J/K navigate · G regroup · TAB review.
export function Booth({
  episodeDir,
  session,
  onSession,
  onRegroup,
  onReview,
  onBack,
}: {
  episodeDir: string;
  session: Session;
  onSession: (s: Session) => void;
  onRegroup: () => void;
  onReview: () => void;
  onBack: () => void;
}) {
  const [recording, setRecording] = useState(false);
  const [recStart, setRecStart] = useState(0);
  const [clock, setClock] = useState("00:00.0");
  const [device, setDevice] = useState<DeviceInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revertArmed, setRevertArmed] = useState(false);
  const [undo, setUndo] = useState<{ passage: number; take: Take } | null>(null);
  const [playing, setPlaying] = useState(false);
  const [recFlash, setRecFlash] = useState(0);
  const { frame, lastAt } = useAudioFrames();
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const revertTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const undoTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const busy = useRef(false); // serialize start/stop invokes

  const cursor = session.cursor;
  const passage = session.passages[cursor];
  const chapterIdx = (() => {
    let n = 0;
    const seen = new Set<string>();
    for (let i = 0; i <= cursor && i < session.passages.length; i++) {
      const ch = passageChapter(session, session.passages[i]);
      if (!seen.has(ch)) {
        seen.add(ch);
        if (i <= cursor) n = seen.size;
      }
    }
    return n;
  })();
  const chapterTotal = new Set(
    session.passages.map((p) => passageChapter(session, p)),
  ).size;

  useEffect(() => {
    currentDevice().then(setDevice).catch(() => setDevice(null));
  }, [recording]);

  useEffect(() => {
    if (!recording) return;
    const t = setInterval(() => setClock(fmtClock(Date.now() - recStart)), 100);
    return () => clearInterval(t);
  }, [recording, recStart]);

  // gap fix #4 — a dead/denied mic records silence with no warning otherwise
  const [noSignal, setNoSignal] = useState(false);
  useEffect(() => {
    if (!recording) {
      setNoSignal(false);
      return;
    }
    let maxPeak = 0;
    const t = setInterval(() => {
      maxPeak = Math.max(maxPeak, frame.current?.peak ?? 0);
      setNoSignal(Date.now() - recStart > 2000 && maxPeak < 0.001);
    }, 250);
    return () => clearInterval(t);
  }, [recording, recStart, frame]);

  const setSessionAndSave = (s: Session) => {
    onSession(s);
    void saveSession(episodeDir, s);
  };

  const stopPlayback = () => {
    audioRef.current?.pause();
    audioRef.current = null;
    setPlaying(false);
  };

  // revert-arm and undo-window are per-passage intents: ANY context change
  // (navigate, record, accept, leave) disarms them — DESIGN.md gap fixes #1/#2
  const disarmRevert = () => {
    if (revertTimer.current) clearTimeout(revertTimer.current);
    setRevertArmed(false);
  };
  const cancelUndo = () => {
    if (undoTimer.current) clearTimeout(undoTimer.current);
    setUndo(null);
  };

  const toggleRecord = async () => {
    if (busy.current) return;
    busy.current = true;
    try {
      if (!recording) {
        stopPlayback();
        disarmRevert();
        if (undo?.passage === cursor) cancelUndo(); // gap fix #2
        setError(null);
        playSfx("toggle", 0.4); // before the stream opens — never bleeds
        setRecordingGate(true);
        await startRecording(episodeDir, cursor);
        setRecStart(Date.now());
        setClock("00:00.0");
        setRecording(true);
        setRecFlash((f) => f + 1); // going hot

      } else {
        const s = await stopRecording();
        setRecording(false);
        setRecordingGate(false);
        onSession(s);
        playSfx("ping", 0.45); // new take card lands
      }
    } catch (e) {
      setRecording(false);
      setRecordingGate(false);
      setError(String(e));
      playSfx("error", 0.4);
    } finally {
      busy.current = false;
    }
  };

  const playTop = async () => {
    if (recording || !passage) return;
    if (playing) return stopPlayback();
    const t = topTake(passage);
    if (!t) return playSfx("error", 0.3);
    const path = await takePath(episodeDir, t.file);
    const el = new Audio(`${convertFileSrc(path)}?t=${Date.now()}`);
    audioRef.current = el;
    el.onended = () => setPlaying(false);
    setPlaying(true);
    void el.play().catch(() => setPlaying(false));
  };

  const revert = async () => {
    if (recording || !passage || passage.takes.length === 0)
      return playSfx("error", 0.3);
    if (!revertArmed) {
      setRevertArmed(true);
      playSfx("nav", 0.35);
      revertTimer.current = setTimeout(() => setRevertArmed(false), 600);
      return;
    }
    if (revertTimer.current) clearTimeout(revertTimer.current);
    setRevertArmed(false);
    stopPlayback();
    try {
      const [s, take] = await discardTake(episodeDir, cursor);
      onSession(s);
      playSfx("toggle", 0.45);
      setUndo({ passage: cursor, take });
      if (undoTimer.current) clearTimeout(undoTimer.current);
      undoTimer.current = setTimeout(() => setUndo(null), 5000);
    } catch (e) {
      setError(String(e));
      playSfx("error", 0.4);
    }
  };

  // ✕ on a take card (founder 2026-06-12): discard ANY take, not just the top.
  // Same disk semantics as revert — the file moves to discarded/, never deleted.
  const deleteTakeAt = async (index: number) => {
    if (recording || !passage) return;
    if (index === passage.takes.length - 1) stopPlayback(); // deleting what's playing
    disarmRevert(); // the stack is changing under the armed intent
    try {
      const [s, take] = await discardTakeAt(episodeDir, cursor, index);
      onSession(s);
      playSfx("toggle", 0.45);
      setUndo({ passage: cursor, take }); // note: undo restores to TOP of stack
      if (undoTimer.current) clearTimeout(undoTimer.current);
      undoTimer.current = setTimeout(() => setUndo(null), 5000);
    } catch (e) {
      setError(String(e));
      playSfx("error", 0.4);
    }
  };

  const doUndo = async () => {
    if (!undo) return;
    try {
      const s = await undoDiscard(episodeDir, undo.passage, undo.take);
      onSession(s);
      setUndo(null);
      playSfx("ping", 0.4);
    } catch (e) {
      setError(String(e));
    }
  };

  // inline transcript edit (founder 2026-06-12) — every saved unit propagates
  // to script-units.json and the canonical completed-videos script.md in Rust;
  // partial-update warnings surface on the amber chip
  const saveEdits = async (updates: { unit: number; text: string }[]) => {
    let s = session;
    const warns: string[] = [];
    for (const u of updates) {
      const [ns, w] = await editUnitText(episodeDir, u.unit, u.text);
      s = ns;
      warns.push(...w);
    }
    onSession(s);
    playSfx("toggle", 0.4);
    setError(warns.length ? `edit saved, but: ${warns.join(" · ")}` : null);
  };

  const accept = () => {
    if (recording || !passage) return;
    if (passage.takes.length === 0) return playSfx("error", 0.3);
    const passages = session.passages.map((p, i) =>
      i === cursor ? { ...p, accepted: true } : p,
    );
    const nextCursor = Math.min(cursor + 1, session.passages.length - 1);
    stopPlayback();
    disarmRevert();
    playSfx("toggle", 0.45);
    setSessionAndSave({ ...session, passages, cursor: nextCursor });
  };

  const nav = (d: number) => {
    if (recording) return;
    stopPlayback();
    disarmRevert(); // gap fix #1 — never carry an armed revert across passages
    const c = Math.max(0, Math.min(session.passages.length - 1, cursor + d));
    if (c !== cursor) {
      playSfx("nav", 0.25);
      setSessionAndSave({ ...session, cursor: c });
    }
  };

  useKeymap(
    {
      space: () => void toggleRecord(),
      p: () => void playTop(),
      r: () => void revert(),
      u: () => void doUndo(),
      enter: accept,
      j: () => nav(1),
      arrowdown: () => nav(1),
      k: () => nav(-1),
      arrowup: () => nav(-1),
      g: () => {
        if (!recording) onRegroup();
      },
      tab: () => {
        if (!recording) onReview();
      },
      escape: () => {
        if (!recording) {
          stopPlayback();
          disarmRevert();
          playSfx("nav", 0.3);
          onBack();
        }
      },
    },
    [session, recording, revertArmed, undo, playing],
  );

  const recorded = session.passages.filter((p) => p.takes.length > 0).length;

  return (
    <div className="screen" style={{ padding: "56px 70px 28px" }}>
      {/* top rail */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 18,
          color: "var(--dim-cyan)",
          fontSize: 11,
          letterSpacing: "0.3em",
        }}
      >
        <span style={{ whiteSpace: "nowrap" }}>
          {session.episode.split("-")[0].toUpperCase()} ▸ CH{" "}
          {String(chapterIdx).padStart(2, "0")}/
          {String(chapterTotal).padStart(2, "0")} ▸ PASSAGE{" "}
          {String(cursor + 1).padStart(2, "0")}/{session.passages.length}
        </span>
        <span style={{ flex: 1 }} />
        <span style={{ letterSpacing: "0.1em", opacity: 0.8 }}>
          {device
            ? `${device.name} · ${(session.format?.sampleRate ?? device.sampleRate) / 1000}k/24`
            : "—"}
        </span>
        <LevelMeter frame={frame} recording={recording} />
      </div>

      {/* teleprompter */}
      <Teleprompter
        session={session}
        cursor={cursor}
        editable={!recording}
        onSaveEdits={saveEdits}
      />

      {/* oscilloscope strip */}
      <div style={{ position: "relative", margin: "10px 0 14px" }}>
        <Oscilloscope
          frame={frame}
          lastAt={lastAt}
          recording={recording}
          height={110}
        />
        {recording && (
          <div
            style={{
              position: "absolute",
              left: 4,
              top: 4,
              fontSize: 12,
              letterSpacing: "0.2em",
            }}
          >
            <GlitchFlash fire={recFlash}>
              <span
                style={{
                  display: "inline-flex",
                  gap: 10,
                  alignItems: "center",
                  color: "var(--amber)",
                }}
              >
                <span
                  style={{
                    width: 8,
                    height: 8,
                    borderRadius: "50%",
                    background: "var(--red)",
                    boxShadow: "0 0 8px var(--red)",
                  }}
                />
                REC {clock}
                {noSignal && (
                  <span style={{ color: "var(--red)", letterSpacing: "0.15em" }}>
                    ⚠ NO SIGNAL — CHECK INPUT / MIC PERMISSION
                  </span>
                )}
              </span>
            </GlitchFlash>
          </div>
        )}
      </div>

      {/* control bar — every action clickable; fluid metrics so the full row
          fits at any window width (hints hide first when narrow) */}
      <div className="control-bar">
        <Btn
          id="episodes"
          label="‹ Episodes"
          hint="esc"
          disabled={recording}
          onClick={() => {
            if (!recording) {
              stopPlayback();
              disarmRevert();
              onBack();
            }
          }}
        />
        <Btn
          id="prev"
          label="‹ Prev"
          hint="k"
          disabled={recording || cursor === 0}
          onClick={() => nav(-1)}
        />
        <Btn
          id="next"
          label="Next ›"
          hint="j"
          disabled={recording || cursor >= session.passages.length - 1}
          onClick={() => nav(1)}
        />
        <span style={{ flex: 1 }} />
        <RecBtn recording={recording} onClick={() => void toggleRecord()} />
        <span style={{ flex: 1 }} />
        <Btn
          id="play"
          label={playing ? "■ Stop" : "▶ Play"}
          hint="p"
          disabled={recording || !passage || passage.takes.length === 0}
          onClick={() => void playTop()}
        />
        <Btn
          id="revert"
          label={revertArmed ? "Confirm ↩" : "↩ Revert"}
          hint="r·r"
          variant="danger"
          disabled={recording || !passage || passage.takes.length === 0}
          onClick={() => void revert()}
        />
        <Btn
          id="accept"
          label="Accept ▸"
          hint="⏎"
          variant="success"
          disabled={recording || !passage || passage.takes.length === 0}
          onClick={accept}
        />
        <span style={{ width: 18 }} />
        <Btn
          id="transcript"
          label="View Transcript"
          hint="g"
          disabled={recording}
          onClick={() => {
            if (!recording) onRegroup();
          }}
        />
        <Btn
          id="review"
          label="Review"
          hint="⇥"
          disabled={recording}
          onClick={() => {
            if (!recording) onReview();
          }}
        />
      </div>

      {/* bottom: take stack + status */}
      <div style={{ display: "flex", gap: 32, alignItems: "flex-start" }}>
        {passage && (
          <TakeStack
            passage={passage}
            revertArmed={revertArmed}
            playing={playing}
            disabled={recording}
            onDelete={(i) => void deleteTakeAt(i)}
          />
        )}
        <div style={{ flex: 1 }}>
          {error && (
            <div
              style={{
                color: "var(--amber)",
                fontSize: 12,
                marginBottom: 8,
                lineHeight: 1.5,
              }}
            >
              ⚠ {error}
            </div>
          )}
          {/* undo lives HERE, not in the control bar — a conditional button up
              there widened the bar past the window edge and clipped REVIEW */}
          {undo && (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 12,
                marginBottom: 8,
              }}
            >
              <Btn
                id="undo"
                label="↶ Undo"
                hint="u"
                variant="danger"
                onClick={() => void doUndo()}
              />
              <span style={{ color: "var(--amber)", fontSize: 12 }}>
                TAKE DISCARDED — UNDO WITHIN 5s
              </span>
            </div>
          )}
          <div
            style={{
              color: "var(--faint-cyan)",
              fontSize: 11,
              letterSpacing: "0.15em",
              marginBottom: 8,
            }}
          >
            {recorded}/{session.passages.length} RECORDED
          </div>
          {recorded === session.passages.length && !recording && (
            <Btn
              id="complete"
              label="All passages recorded — Review ▸ Export"
              variant="success"
              onClick={onReview}
            />
          )}
        </div>
      </div>
    </div>
  );
}
