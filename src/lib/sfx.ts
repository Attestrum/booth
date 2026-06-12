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

export function playSfx(name: SfxName, volume = 0.5) {
  if (recordingGate) return; // never bleed into the mic
  let el = pool.get(name);
  if (!el) {
    el = new Audio(SOURCES[name]);
    pool.set(name, el);
  }
  el.volume = volume;
  el.currentTime = 0;
  void el.play().catch(() => {}); // pre-gesture autoplay denial is fine
}
