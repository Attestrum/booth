import { useEffect, useRef } from "react";
import { onAudioFrame, type AudioFrame } from "../lib/ipc";

// Latest mic frame in a ref (30 Hz from Rust while recording) — consumers read
// it inside their own rAF loop; no React re-render per frame.
export function useAudioFrames() {
  const frame = useRef<AudioFrame | null>(null);
  const lastAt = useRef(0);
  useEffect(() => {
    const un = onAudioFrame((f) => {
      frame.current = f;
      lastAt.current = performance.now();
    });
    return () => {
      un.then((f) => f());
    };
  }, []);
  return { frame, lastAt };
}
