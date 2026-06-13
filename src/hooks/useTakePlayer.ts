import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { takePath } from "../lib/ipc";
import { clipSpans, keptSpans, type Take } from "../lib/session";

// Gapless, click-free playback of a take's KEPT spans (cuts skipped), via the
// Web Audio API instead of seeking an <audio> element's currentTime.
//
// Why: jumping currentTime at each cut restarts the decoder (audible click) and
// is driven by the coarse ~4 Hz `timeupdate` event (visual stutter). Here we
// decode the WAV once into an AudioBuffer and schedule each kept span as a
// back-to-back AudioBufferSourceNode on the AudioContext clock — sample-accurate
// and gapless — with a short gain ramp at every span edge so splices don't click
// (the standard Web Audio anti-click fade). The playhead is read from the audio
// clock via rAF, so it's smooth (60 fps) and sample-precise.

const FADE = 0.006; // s — inaudible edge ramp that removes the splice click
const LEAD = 0.03; // s — schedule slightly ahead of the clock

type Seg = { s: number; keptStart: number; keptEnd: number }; // s = original sec

export function useTakePlayer(episodeDir: string) {
  const [playing, setPlaying] = useState(false);
  const playheadRef = useRef(-1); // current ORIGINAL second, -1 when idle

  const ctxRef = useRef<AudioContext | null>(null);
  const cacheRef = useRef(new Map<string, AudioBuffer>());
  const bufRef = useRef<AudioBuffer | null>(null);
  const nodesRef = useRef<{ src: AudioBufferSourceNode; g: GainNode }[]>([]);
  const segRef = useRef<{ segments: Seg[]; total: number }>({ segments: [], total: 0 });
  const baseRef = useRef(0); // kept-offset this schedule started from
  const startCtxRef = useRef(0); // ctx time the schedule was anchored to
  const pausedRef = useRef(0); // kept-offset captured at pause
  const statusRef = useRef<"idle" | "playing" | "paused">("idle");
  const rafRef = useRef(0);

  const ensureCtx = () => {
    if (!ctxRef.current) ctxRef.current = new AudioContext();
    if (ctxRef.current.state === "suspended") void ctxRef.current.resume();
    return ctxRef.current;
  };

  const loadBuffer = async (file: string) => {
    const cached = cacheRef.current.get(file);
    if (cached) return cached;
    const path = await takePath(episodeDir, file);
    const res = await fetch(convertFileSrc(path));
    const arr = await res.arrayBuffer();
    const buf = await ensureCtx().decodeAudioData(arr);
    cacheRef.current.set(file, buf);
    return buf;
  };

  // current position along the collapsed (kept) timeline
  const currentKept = () => {
    const ctx = ctxRef.current;
    if (!ctx) return 0;
    const ko = baseRef.current + Math.max(0, ctx.currentTime - startCtxRef.current);
    return Math.min(ko, segRef.current.total);
  };
  // kept-offset → original second (for the playhead)
  const origAt = (ko: number) => {
    const segs = segRef.current.segments;
    for (const seg of segs) if (ko <= seg.keptEnd) return seg.s + (ko - seg.keptStart);
    const last = segs[segs.length - 1];
    return last ? last.s + (last.keptEnd - last.keptStart) : 0;
  };

  const stopNodes = () => {
    for (const { src, g } of nodesRef.current) {
      try {
        src.onended = null;
        src.stop();
      } catch {
        /* already stopped */
      }
      try {
        src.disconnect();
        g.disconnect();
      } catch {
        /* noop */
      }
    }
    nodesRef.current = [];
  };

  const cancelRaf = () => {
    if (rafRef.current) cancelAnimationFrame(rafRef.current);
    rafRef.current = 0;
  };

  const stop = useCallback(() => {
    stopNodes();
    cancelRaf();
    statusRef.current = "idle";
    playheadRef.current = -1;
    setPlaying(false);
  }, []);

  const tick = useCallback(() => {
    const ko = currentKept();
    if (ko >= segRef.current.total - 0.005) {
      stop();
      return;
    }
    playheadRef.current = origAt(ko);
    rafRef.current = requestAnimationFrame(tick);
  }, [stop]);

  const scheduleFrom = (base: number) => {
    const ctx = ctxRef.current!;
    const startCtx = ctx.currentTime + LEAD;
    baseRef.current = base;
    startCtxRef.current = startCtx;
    stopNodes();
    for (const seg of segRef.current.segments) {
      if (seg.keptEnd <= base + 1e-6) continue;
      const segBase = Math.max(seg.keptStart, base);
      const when = startCtx + (segBase - base);
      const offset = seg.s + (segBase - seg.keptStart);
      const d = seg.keptEnd - segBase;
      if (d <= 0) continue;
      const src = ctx.createBufferSource();
      src.buffer = bufRef.current;
      const g = ctx.createGain();
      src.connect(g);
      g.connect(ctx.destination);
      const fade = Math.min(FADE, d / 2);
      g.gain.setValueAtTime(0, when);
      g.gain.linearRampToValueAtTime(1, when + fade);
      g.gain.setValueAtTime(1, Math.max(when + fade, when + d - fade));
      g.gain.linearRampToValueAtTime(0, when + d);
      src.start(when, offset, d);
      src.stop(when + d + 0.02);
      nodesRef.current.push({ src, g });
    }
  };

  // Play / Pause / Resume. Returns false only when there's nothing audible to
  // play (so the caller can sound an error). Pause/resume return true.
  const toggle = useCallback(
    async (take: Take, fromSec: number, toSec: number | null): Promise<boolean> => {
      ensureCtx();
      if (statusRef.current === "playing") {
        pausedRef.current = currentKept();
        stopNodes();
        cancelRaf();
        statusRef.current = "paused";
        setPlaying(false);
        return true;
      }
      if (statusRef.current === "paused") {
        scheduleFrom(pausedRef.current);
        statusRef.current = "playing";
        setPlaying(true);
        rafRef.current = requestAnimationFrame(tick);
        return true;
      }
      // fresh
      const spans = clipSpans(keptSpans(take), fromSec, toSec ?? (take.durationSec || 0));
      if (!spans.length) return false;
      let cum = 0;
      const segments = spans.map(([s, e]) => {
        const seg = { s, keptStart: cum, keptEnd: cum + (e - s) };
        cum += e - s;
        return seg;
      });
      segRef.current = { segments, total: cum };
      bufRef.current = await loadBuffer(take.file);
      scheduleFrom(0);
      statusRef.current = "playing";
      playheadRef.current = spans[0][0];
      setPlaying(true);
      rafRef.current = requestAnimationFrame(tick);
      return true;
    },
    // refs + stable setters only; episodeDir is the lone reactive input
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [episodeDir, tick],
  );

  useEffect(() => {
    const ctx = ctxRef.current;
    return () => {
      stop();
      void ctx?.close();
    };
  }, [stop]);

  return { playing, playheadRef, toggle, stop };
}
