import { useEffect, useRef, useState } from "react";
import { playSfx } from "../lib/sfx";
import { GlitchFlash } from "../components/GlitchFlash";

// Screen 0 — the CRT ritual. PRESS ANY KEY (the keypress unlocks WKWebView
// audio) -> 1.4 s power-on synced to sfx-monitor-power-on.mp3:
//   0-250ms   horizontal cyan line snaps across center
//   250-650ms line blooms vertically into the full screen
//   650ms     wordmark prints + one RGB-split glitch
//   1400ms    settle -> onDone
// Single pass -> settle. The prompt breathes (seamless loop) while waiting.
export function PowerOn({ onDone }: { onDone: () => void }) {
  const [phase, setPhase] = useState<"waiting" | "line" | "bloom" | "mark">(
    "waiting",
  );
  const [glitch, setGlitch] = useState(0);
  const fired = useRef(false);

  useEffect(() => {
    const fire = () => {
      if (fired.current) return;
      fired.current = true;
      playSfx("powerOn", 0.6);
      setPhase("line");
      setTimeout(() => setPhase("bloom"), 250);
      setTimeout(() => {
        setPhase("mark");
        setGlitch((g) => g + 1);
      }, 650);
      setTimeout(onDone, 1500);
    };
    window.addEventListener("keydown", fire);
    window.addEventListener("mousedown", fire);
    return () => {
      window.removeEventListener("keydown", fire);
      window.removeEventListener("mousedown", fire);
    };
  }, [onDone]);

  return (
    <div
      className="screen"
      style={{
        alignItems: "center",
        justifyContent: "center",
        cursor: phase === "waiting" ? "pointer" : "default",
      }}
    >
      {phase === "waiting" && (
        <div
          style={{
            color: "var(--dim-cyan)",
            letterSpacing: "0.42em",
            fontSize: 14,
            animation: "booth-breathe 3s ease-in-out infinite",
          }}
        >
          PRESS ANY KEY
        </div>
      )}

      {(phase === "line" || phase === "bloom") && (
        <div
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            top: "50%",
            height: 2,
            background: "var(--cyan)",
            boxShadow: "0 0 24px var(--cyan), 0 0 60px var(--dim-cyan)",
            transformOrigin: "center",
            animation:
              phase === "line"
                ? "booth-linesnap 250ms cubic-bezier(0.2, 0.9, 0.2, 1) forwards"
                : "booth-bloom 400ms ease-out forwards",
          }}
        />
      )}

      {phase === "mark" && (
        <GlitchFlash fire={glitch}>
          <div
            style={{
              color: "var(--cyan)",
              letterSpacing: "0.42em",
              fontSize: 24,
              textShadow: "0 0 14px var(--dim-cyan)",
            }}
          >
            ATTESTRUM&nbsp;//&nbsp;BOOTH
          </div>
        </GlitchFlash>
      )}

      <style>{`
        @keyframes booth-breathe { 0%,100% { opacity: 0.5 } 50% { opacity: 1 } }
        @keyframes booth-linesnap { from { transform: scaleX(0) } to { transform: scaleX(1) } }
        @keyframes booth-bloom {
          from { transform: scaleY(1); opacity: 1 }
          to   { transform: scaleY(220); opacity: 0 }
        }
      `}</style>
    </div>
  );
}
