import { useEffect } from "react";

/// One global keydown dispatcher per screen. Keys: "j", "enter", "cmd+e",
/// "cmd+shift+e", "space"…
export function useKeymap(
  map: Record<string, (e: KeyboardEvent) => void>,
  deps: unknown[],
) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // typing in an editor (inline transcript edit) must never fire booth keys
      const t = e.target as HTMLElement | null;
      if (
        t &&
        (t.tagName === "TEXTAREA" || t.tagName === "INPUT" || t.isContentEditable)
      ) {
        return;
      }
      const key =
        (e.metaKey ? "cmd+" : "") +
        (e.metaKey && e.shiftKey ? "shift+" : "") +
        (e.key === " " ? "space" : e.key.toLowerCase());
      const fn = map[key];
      if (fn) {
        e.preventDefault();
        fn(e);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
