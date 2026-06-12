import { useEffect } from "react";

// DEV-ONLY test driver: polls /autopilot.json (booth/public/autopilot.json) and
// replays its keys as window KeyboardEvents, exactly like a physical keyboard.
// Synthetic System Events keystrokes never reach the WKWebView, so automated
// verification drives the app through this instead. No-op in production builds.
//
// protocol: {"seq": <int>, "keys": ["space","wait:1500","j","click:rec", ...]}
// a new seq value replays the whole keys list (300 ms apart; "wait:N" inserts
// an extra N ms pause; "click:<id>" clicks the element with data-autopilot=id).
export function useAutopilot() {
  useEffect(() => {
    if (!import.meta.env.DEV) return;
    let lastSeq = -1;
    let timer: ReturnType<typeof setInterval> | null = null;

    const press = (spec: string) => {
      if (spec.startsWith("click:")) {
        const el = document.querySelector<HTMLElement>(
          `[data-autopilot="${spec.slice(6)}"]`,
        );
        el?.click();
        return;
      }
      const meta = spec.startsWith("cmd+");
      const raw = meta ? spec.slice(4) : spec;
      const key =
        raw === "space" ? " " : raw === "enter" ? "Enter" : raw;
      window.dispatchEvent(
        new KeyboardEvent("keydown", { key, metaKey: meta, bubbles: true }),
      );
    };

    timer = setInterval(async () => {
      try {
        const res = await fetch(`/autopilot.json?t=${Date.now()}`, {
          cache: "no-store",
        });
        if (!res.ok) return;
        const { seq, keys } = await res.json();
        if (typeof seq !== "number" || seq === lastSeq) return;
        if (lastSeq === -1 && seq >= 0) {
          // first sight: record baseline, don't replay stale commands
          lastSeq = seq;
          return;
        }
        lastSeq = seq;
        let at = 0;
        for (const k of keys as string[]) {
          if (k.startsWith("wait:")) {
            at += Number(k.slice(5)) || 0;
            continue;
          }
          at += 300;
          setTimeout(() => press(k), at);
        }
      } catch {
        /* file absent — idle */
      }
    }, 400);
    return () => {
      if (timer) clearInterval(timer);
    };
  }, []);
}
