// The one button vocabulary (DESIGN.md gap fix #9): bordered chip, caps,
// embedded key hint, hover glow, obvious cursor. Buttons fire the SAME handlers
// as their keys. `id` doubles as the dev-autopilot click target.
export function Btn({
  id,
  label,
  hint,
  onClick,
  variant = "primary",
  disabled = false,
}: {
  id: string;
  label: string;
  hint?: string;
  onClick: () => void;
  variant?: "primary" | "danger" | "success";
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className={`btn btn--${variant}`}
      data-autopilot={id}
      disabled={disabled}
      onClick={onClick}
    >
      {label}
      {hint && <span className="btn-hint">{hint}</span>}
    </button>
  );
}

// The marquee control: record/stop glyph + label, no ring (founder 2026-06-12
// removed the circle). The glyph's glow pulses (seamless loop — the one
// continuous animation, justified as a real-device REC idiom) while recording.
export function RecBtn({
  recording,
  onClick,
}: {
  recording: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`rec-btn ${recording ? "rec-btn--live" : ""}`}
      data-autopilot="rec"
      onClick={onClick}
      title={recording ? "Stop (R)" : "Record (R)"}
    >
      <span className="rec-btn-glyph">{recording ? "■" : "●"}</span>
      <span className="rec-btn-label">
        {recording ? "STOP" : "REC"} <span className="btn-hint">R</span>
      </span>
    </button>
  );
}
