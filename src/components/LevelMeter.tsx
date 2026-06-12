import { useEffect, useRef, useState } from "react";
import type { AudioFrame } from "../lib/ipc";

const SEGS = 24;

// Segmented input meter: cyan body, amber above −6 dBFS, red latch on clip.
export function LevelMeter({
  frame,
  recording,
}: {
  frame: React.MutableRefObject<AudioFrame | null>;
  recording: boolean;
}) {
  const [lit, setLit] = useState(0);
  const [clip, setClip] = useState(false);
  const clipUntil = useRef(0);

  useEffect(() => {
    let raf = 0;
    const tick = () => {
      const f = frame.current;
      if (recording && f) {
        const db = 20 * Math.log10(Math.max(f.peak, 1e-5));
        const norm = Math.max(0, Math.min(1, (db + 50) / 50)); // -50dB..0dB
        setLit(Math.round(norm * SEGS));
        if (f.clip) clipUntil.current = performance.now() + 900;
        setClip(performance.now() < clipUntil.current);
      } else {
        setLit(0);
        setClip(false);
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [frame, recording]);

  // segment index where -6 dBFS sits: (-6+50)/50 of the scale
  const amberFrom = Math.round(((-6 + 50) / 50) * SEGS);

  return (
    <div style={{ display: "flex", gap: 3, alignItems: "center" }}>
      {Array.from({ length: SEGS }, (_, i) => {
        const on = i < lit;
        const color = clip
          ? "var(--red)"
          : i >= amberFrom
            ? "var(--amber)"
            : "var(--cyan)";
        return (
          <div
            key={i}
            style={{
              width: 5,
              height: 14,
              background: on ? color : "var(--faint-cyan)",
              boxShadow: on ? `0 0 6px ${color}` : "none",
            }}
          />
        );
      })}
    </div>
  );
}
