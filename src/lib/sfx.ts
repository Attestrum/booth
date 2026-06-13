// UI SFX, bundled with the app under src/assets/sfx/.
// CORRECTNESS RULE: every sound is hard-muted while the mic stream is open —
// UI sounds must never bleed into a recorded take.
import uiNav from "../assets/sfx/sfx-ui-nav.mp3";
import uiToggle from "../assets/sfx/sfx-ui-toggle.mp3";
import nodePing from "../assets/sfx/sfx-node-ping.mp3";
import errorSfx from "../assets/sfx/sfx-error.mp3";
import verifiedChime from "../assets/sfx/sfx-verified-chime.mp3";
import powerOn from "../assets/sfx/sfx-monitor-power-on.mp3";
import tick from "../assets/sfx/sfx-typewriter-tick-one-click.mp3";

const SOURCES = {
  nav: uiNav,
  toggle: uiToggle,
  ping: nodePing,
  error: errorSfx,
  chime: verifiedChime,
  powerOn: powerOn,
  tick: tick,
} as const;

export type SfxName = keyof typeof SOURCES;

let recordingGate = false;
export const setRecordingGate = (armed: boolean) => {
  recordingGate = armed;
};

const pool = new Map<SfxName, HTMLAudioElement>();

// Resolves when the cue finishes (or is denied) — callers that must not let a
// cue bleed into the mic (record start) can `await` it before opening the stream.
export function playSfx(name: SfxName, volume = 0.5): Promise<void> {
  if (recordingGate) return Promise.resolve(); // never bleed into the mic
  let el = pool.get(name);
  if (!el) {
    el = new Audio(SOURCES[name]);
    pool.set(name, el);
  }
  const sfx = el;
  sfx.volume = volume;
  sfx.currentTime = 0;
  return new Promise<void>((resolve) => {
    let settled = false;
    const finish = () => {
      if (settled) return;
      settled = true;
      sfx.removeEventListener("ended", finish);
      resolve();
    };
    sfx.addEventListener("ended", finish, { once: true });
    // safety net: never block the caller longer than the clip itself
    const ms =
      (Number.isFinite(sfx.duration) && sfx.duration > 0 ? sfx.duration * 1000 : 800) + 120;
    window.setTimeout(finish, ms);
    void sfx.play().catch(() => finish()); // pre-gesture autoplay denial is fine
  });
}
