import { useEffect, useState } from "react";

const ROWS: [string, string][] = [
  ["SPACE", "record / stop"],
  ["P", "play top take"],
  ["R R", "revert top take (double-tap — moves to discarded/, never deletes)"],
  ["✕ on a take card", "delete that take (single click; 5 s undo)"],
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
        ?
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
            color: "var(--faint-cyan)",
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
