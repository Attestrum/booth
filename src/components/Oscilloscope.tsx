import { useEffect, useRef } from "react";
import type { AudioFrame } from "../lib/ipc";

// The live mic trace. Idle: near-flat dual-sine breathing line (WaveformLines
// mood — a monitor at rest, seamless loop). Recording: the real 128-sample
// window, bright with bloom. Canvas2D in a rAF loop, no React re-renders.
export function Oscilloscope({
  frame,
  lastAt,
  recording,
  height = 120,
}: {
  frame: React.MutableRefObject<AudioFrame | null>;
  lastAt: React.MutableRefObject<number>;
  recording: boolean;
  height?: number;
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const recRef = useRef(recording);
  recRef.current = recording;

  useEffect(() => {
    const canvas = canvasRef.current!;
    const ctx = canvas.getContext("2d")!;
    let raf = 0;
    // smoothed display window so 30 Hz frames look fluid at 60 fps
    let display: number[] = new Array(128).fill(0);

    const draw = (t: number) => {
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

      const live =
        recRef.current &&
        frame.current &&
        performance.now() - lastAt.current < 250;

      if (live) {
        const target = frame.current!.window;
        for (let i = 0; i < 128; i++) {
          display[i] += (target[i] - display[i]) * 0.55;
        }
        ctx.strokeStyle = "#7FE0FF";
        ctx.lineWidth = 2.2;
        ctx.shadowColor = "rgba(127,224,255,0.8)";
        ctx.shadowBlur = 12;
      } else {
        // idle: seamless dual-sine breathe, amplitude whispers
        const s = t / 1000;
        for (let i = 0; i < 128; i++) {
          const x = i / 128;
          display[i] =
            Math.sin(x * 9.3 + s * 1.1) * 0.018 +
            Math.sin(x * 23.7 - s * 0.7) * 0.009;
        }
        ctx.strokeStyle = "rgba(127,224,255,0.45)";
        ctx.lineWidth = 1.4;
        ctx.shadowColor = "rgba(127,224,255,0.45)";
        ctx.shadowBlur = 8;
      }

      ctx.beginPath();
      for (let i = 0; i < 128; i++) {
        const x = (i / 127) * w;
        const y = mid - display[i] * (h * 0.46);
        i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
      }
      ctx.stroke();
      raf = requestAnimationFrame(draw);
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [frame, lastAt]);

  return (
    <canvas
      ref={canvasRef}
      style={{ width: "100%", height, display: "block" }}
    />
  );
}
