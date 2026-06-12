import { useEffect, useState } from "react";

// One-shot RGB-split glitch (IntroCard idiom): cyan main + pink/yellow ghosts
// offset ±4px with snap jitter, fires once per `fire` increment then settles.
// Single pass -> settle, per the motion law. Children must render identically
// in all three layers (pure text/markup).
export function GlitchFlash({
  fire,
  children,
}: {
  fire: number;
  children: React.ReactNode;
}) {
  const [active, setActive] = useState(false);
  const [jitter, setJitter] = useState({ x: 0, y: 0 });

  useEffect(() => {
    if (fire === 0) return;
    setActive(true);
    // deterministic snap-jitter: two snaps then settle
    const snaps = [
      { at: 0, x: 2, y: -1 },
      { at: 80, x: -2, y: 1 },
      { at: 160, x: 1, y: 0 },
    ];
    const timers = snaps.map((s) =>
      setTimeout(() => setJitter({ x: s.x, y: s.y }), s.at),
    );
    const end = setTimeout(() => {
      setActive(false);
      setJitter({ x: 0, y: 0 });
    }, 240);
    return () => {
      timers.forEach(clearTimeout);
      clearTimeout(end);
    };
  }, [fire]);

  return (
    <span
      style={{
        position: "relative",
        display: "inline-block",
        transform: `translate(${jitter.x}px, ${jitter.y}px)`,
      }}
    >
      {active && (
        <>
          <span
            aria-hidden
            style={{
              position: "absolute",
              inset: 0,
              transform: "translate(4px, 0)",
              filter: "hue-rotate(-150deg) saturate(2)",
              opacity: 0.8,
              pointerEvents: "none",
            }}
          >
            {children}
          </span>
          <span
            aria-hidden
            style={{
              position: "absolute",
              inset: 0,
              transform: "translate(-4px, 0)",
              filter: "hue-rotate(140deg) saturate(2)",
              opacity: 0.8,
              pointerEvents: "none",
            }}
          >
            {children}
          </span>
        </>
      )}
      <span style={{ position: "relative" }}>{children}</span>
    </span>
  );
}
