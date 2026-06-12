// Attestrum brand constants — mirrored from pipeline/src/theme.ts (Remotion-free).
// Source of truth is the landing page aesthetic; do not invent new hues.
export const THEME = {
  cyan: "#7FE0FF",
  bg: "#0A0E14",
  verifiedGreen: "#7FFFB0",
  dimCyan: "rgba(127, 224, 255, 0.45)",
  faintCyan: "rgba(127, 224, 255, 0.12)",
  amber: "#E0A52E",
  red: "#FF6B6B",
  mono: `'IBM Plex Mono', 'JetBrains Mono', 'Courier New', monospace`,
} as const;

export const glow = (px = 8, color = THEME.dimCyan) =>
  `drop-shadow(0 0 ${px}px ${color})`;

export const textGlow = (px = 12, color = THEME.dimCyan) =>
  `0 0 ${px}px ${color}`;
