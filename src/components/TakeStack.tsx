import type { Passage } from "../lib/session";

const fmt = (s: number) =>
  `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

// The current passage's takes, newest on top. The SELECTED take (the kept one
// that plays / accepts / exports) is the bright/highlighted card — click any row
// to select it. revertArmed paints the NEWEST card amber (R·R discards newest).
// Each card carries a ✕ DELETE (founder 2026-06-12): single click — the 5 s
// undo window is the safety net (file moves to discarded/, never deleted).
export function TakeStack({
  passage,
  selectedIndex,
  revertArmed,
  playing,
  disabled,
  onSelect,
  onDelete,
}: {
  passage: Passage;
  selectedIndex: number;
  revertArmed: boolean;
  playing: boolean;
  disabled: boolean;
  onSelect: (index: number) => void;
  onDelete: (index: number) => void;
}) {
  const takes = [...passage.takes].reverse(); // newest first
  return (
    <div style={{ width: 240 }}>
      <div
        style={{
          color: "var(--dim-cyan)",
          fontSize: 10,
          letterSpacing: "0.3em",
          marginBottom: 8,
          textShadow: "0 0 8px rgba(127, 224, 255, 0.25)",
        }}
      >
        TAKES
      </div>
      {takes.length === 0 && (
        <div style={{ color: "var(--dim-cyan)", fontSize: 12 }}>— none —</div>
      )}
      {takes.map((t, i) => {
        const top = i === 0; // newest — what R·R discards
        const num = passage.takes.length - i;
        const originalIndex = passage.takes.length - 1 - i;
        const selected = originalIndex === selectedIndex; // the kept take
        const cropped = (t.cuts?.length ?? 0) > 0;
        // revert-arm (on the newest) takes visual priority; otherwise the
        // selected card carries the bright border (green once accepted).
        const border =
          top && revertArmed
            ? "var(--amber)"
            : selected
              ? passage.accepted
                ? "var(--green)"
                : "var(--dim-cyan)"
              : "var(--faint-cyan)";
        return (
          <div
            key={t.file}
            role="button"
            aria-pressed={selected}
            data-autopilot={`select-take-${num}`}
            onClick={() => !disabled && !selected && onSelect(originalIndex)}
            style={{
              display: "flex",
              gap: 12,
              alignItems: "center",
              padding: "8px 12px",
              marginBottom: 4,
              border: `1px solid ${border}`,
              borderLeftWidth: 3,
              color: selected ? "var(--cyan)" : "var(--dim-cyan)",
              opacity: selected ? 1 : Math.max(0.4, 0.85 - i * 0.15),
              fontSize: 12,
              textShadow: selected ? "0 0 10px var(--dim-cyan)" : "none",
              cursor: disabled || selected ? "default" : "pointer",
            }}
          >
            <span>T{String(num).padStart(2, "0")}</span>
            <span>{fmt(t.durationSec)}</span>
            {cropped && (
              <span style={{ color: "var(--dim-cyan)", fontSize: 10 }}>✂</span>
            )}
            {t.recovered && (
              <span style={{ color: "var(--amber)", fontSize: 10 }}>
                RECOVERED
              </span>
            )}
            {selected && playing && (
              <span style={{ color: "var(--green)", fontSize: 10 }}>▶</span>
            )}
            {top && revertArmed && (
              <span style={{ color: "var(--amber)", fontSize: 10 }}>
                D AGAIN TO REVERT
              </span>
            )}
            <span style={{ flex: 1 }} />
            <button
              type="button"
              className="take-delete"
              data-autopilot={`delete-take-${num}`}
              title="Delete take (moves to discarded/ — U undoes)"
              disabled={disabled}
              onClick={(e) => {
                e.stopPropagation(); // don't also select the row
                onDelete(originalIndex);
              }}
            >
              ✕
            </button>
          </div>
        );
      })}
    </div>
  );
}
