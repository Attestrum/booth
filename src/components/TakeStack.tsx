import type { Passage } from "../lib/session";

const fmt = (s: number) =>
  `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(Math.floor(s % 60)).padStart(2, "0")}`;

// The current passage's takes as a literal stack — newest on top, brightest.
// revertArmed paints the top card amber (first R of the double-tap).
// Each card carries a ✕ DELETE (founder 2026-06-12): single click — the 5 s
// undo window is the safety net (file moves to discarded/, never deleted).
export function TakeStack({
  passage,
  revertArmed,
  playing,
  disabled,
  onDelete,
}: {
  passage: Passage;
  revertArmed: boolean;
  playing: boolean;
  disabled: boolean;
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
        const top = i === 0;
        const num = passage.takes.length - i;
        const originalIndex = passage.takes.length - 1 - i;
        const border = top
          ? revertArmed
            ? "var(--amber)"
            : passage.accepted
              ? "var(--green)"
              : "var(--dim-cyan)"
          : "var(--faint-cyan)";
        return (
          <div
            key={t.file}
            style={{
              display: "flex",
              gap: 12,
              alignItems: "center",
              padding: "8px 12px",
              marginBottom: 4,
              border: `1px solid ${border}`,
              borderLeftWidth: 3,
              color: top ? "var(--cyan)" : "var(--dim-cyan)",
              opacity: top ? 1 : Math.max(0.35, 0.8 - i * 0.15),
              fontSize: 12,
              textShadow: top ? "0 0 10px var(--dim-cyan)" : "none",
            }}
          >
            <span>T{String(num).padStart(2, "0")}</span>
            <span>{fmt(t.durationSec)}</span>
            {t.recovered && (
              <span style={{ color: "var(--amber)", fontSize: 10 }}>
                RECOVERED
              </span>
            )}
            {top && playing && (
              <span style={{ color: "var(--green)", fontSize: 10 }}>▶</span>
            )}
            {top && revertArmed && (
              <span style={{ color: "var(--amber)", fontSize: 10 }}>
                R AGAIN TO REVERT
              </span>
            )}
            <span style={{ flex: 1 }} />
            <button
              type="button"
              className="take-delete"
              data-autopilot={`delete-take-${num}`}
              title="Delete take (moves to discarded/ — U undoes)"
              disabled={disabled}
              onClick={() => onDelete(originalIndex)}
            >
              ✕
            </button>
          </div>
        );
      })}
    </div>
  );
}
