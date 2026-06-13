// Top-left clickable back affordance for full screens. Sits below the 38px
// titlebar-drag strip and to the right of the macOS traffic lights, with
// app-region:no-drag so the click isn't swallowed for window dragging.
export function BackButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      className="back-link"
      data-autopilot="back"
      onClick={onClick}
    >
      ‹ Back
    </button>
  );
}
