import { useEffect, useState } from "react";

const ROWS: [string, string][] = [
  ["SPACE", "play / pause the selected take"],
  ["R", "record / stop"],
  ["D D", "revert newest take (double-tap — moves to discarded/, never deletes)"],
  ["click waveform", "set the play cursor · drag to select a span"],
  ["DEL", "cut the selection (✕ on a red band restores it)"],
  ["✕ on a take card", "delete that take (single click; 5 s undo)"],
  ["click a take card", "select it as the kept take"],
  ["U", "undo a revert / delete"],
  ["ENTER", "accept take ▸ next passage"],
  ["J / K", "next / previous passage (or row)"],
  ["click the script text", "edit it inline (⌘S saves, Esc cancels)"],
  ["G", "view transcript (M merge · S split)"],
  ["TAB", "review screen"],
  ["⌘E / ⇧⌘E", "export / export partial"],
  ["O / I", "open folder / import script (.md, .txt)"],
  ["ESC", "back"],
];

// ? toggles a modal cheat-sheet on every screen. While open it swallows ALL
// keys (capture-phase) so you can't accidentally record while reading it;
// ? / Esc / click closes.
export function HelpOverlay() {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing =
        t && (t.tagName === "TEXTAREA" || t.tagName === "INPUT" || t.isContentEditable);
      if (!open) {
        if (e.key === "?" && !typing) {
          e.preventDefault();
          setOpen(true);
        }
        return;
      }
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "?" || e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [open]);

  if (!open) {
    return (
      <button
        type="button"
        className="help-hint"
        data-autopilot="help"
        title="Keyboard help (?)"
        onClick={() => setOpen(true)}
      >
        Key Bindings
      </button>
    );
  }

  return (
    <div className="help-overlay" onClick={() => setOpen(false)}>
      <div className="help-panel" onClick={(e) => e.stopPropagation()}>
        <div
          style={{
            color: "var(--dim-cyan)",
            letterSpacing: "0.42em",
            fontSize: 12,
            marginBottom: 18,
          }}
        >
          ATTESTRUM // BOOTH ▸ KEYS
        </div>
        {ROWS.map(([key, what]) => (
          <div
            key={key}
            style={{ display: "flex", gap: 18, padding: "5px 0", fontSize: 12 }}
          >
            <span
              style={{
                width: 170,
                color: "var(--cyan)",
                textShadow: "0 0 8px var(--dim-cyan)",
                flexShrink: 0,
              }}
            >
              {key}
            </span>
            <span style={{ color: "var(--dim-cyan)" }}>{what}</span>
          </div>
        ))}
        <div
          style={{
            color: "var(--dim-cyan)",
            fontSize: 10,
            letterSpacing: "0.2em",
            marginTop: 16,
          }}
        >
          EVERY KEY IS ALSO A VISIBLE BUTTON · ? / ESC / CLICK CLOSES
        </div>
      </div>
    </div>
  );
}
