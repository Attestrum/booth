// Typed wrappers around every Rust command + event. The only IPC surface.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Session, Take } from "./session";
import type {
  Transcript,
  TranscriptSummary,
  TranscribeProgress,
} from "./transcript";

export interface AudioFrame {
  rms: number;
  peak: number;
  clip: boolean;
  window: number[]; // 128 samples, -1..1
}

export interface SessionSummary {
  episodeDir: string;
  episode: string;
  recorded: number;
  total: number;
  takes: number;
}

export interface DeviceInfo {
  name: string;
  sampleRate: number;
}

// ---- projects ----
export const getRecents = () => invoke<string[]>("get_recents");
export const addProject = (dir: string) =>
  invoke<string[]>("add_project", { dir });

// ---- session/script ----
export const scanSessions = (root: string) =>
  invoke<SessionSummary[]>("scan_sessions", { root });
export const listEpisodes = (root: string) =>
  invoke<string[]>("list_episodes", { root });
export const openEpisode = (dir: string) =>
  invoke<{ session: Session; fresh: boolean }>("open_episode", {
    dir,
    nowIso: new Date().toISOString(),
  });
export const saveSession = (dir: string, session: Session) =>
  invoke<void>("save_session", { dir, session });
export const importScript = (path: string) =>
  invoke<{ dir: string; session: Session; fresh: boolean }>("import_script", {
    path,
    nowIso: new Date().toISOString(),
  });

// ---- audio ----
export const currentDevice = () => invoke<DeviceInfo>("current_device");
export const listInputDevices = () => invoke<DeviceInfo[]>("list_input_devices");
export const setInputDevice = (name: string | null) =>
  invoke<DeviceInfo>("set_input_device", { name });
export const startRecording = (dir: string, passage: number) =>
  invoke<string>("start_recording", { dir, passage }); // -> take filename
export const stopRecording = () => invoke<Session>("stop_recording");
export const discardTake = (dir: string, passage: number) =>
  invoke<[Session, Take]>("discard_take", { dir, passage });
export const discardTakeAt = (dir: string, passage: number, index: number) =>
  invoke<[Session, Take]>("discard_take_at", { dir, passage, index });
export const undoDiscard = (dir: string, passage: number, take: Take) =>
  invoke<Session>("undo_discard", { dir, passage, take });
export const editUnitText = (dir: string, unit: number, text: string) =>
  invoke<[Session, string[]]>("edit_unit_text", { dir, unit, text });
export const takePath = (dir: string, file: string) =>
  invoke<string>("take_path", { dir, file });
export const takeWaveform = (dir: string, file: string, buckets: number) =>
  invoke<number[]>("take_waveform", { dir, file, buckets });

// ---- export (P4) ----
export const ffmpegStatus = () => invoke<boolean>("ffmpeg_status");
export const exportSession = (dir: string, allowPartial: boolean) =>
  invoke<{ wav: string; mp3: string | null }>("export_session", {
    dir,
    allowPartial,
  });

export const onAudioFrame = (cb: (f: AudioFrame) => void): Promise<UnlistenFn> =>
  listen<AudioFrame>("audio:frame", (e) => cb(e.payload));

export const onExportProgress = (
  cb: (msg: string) => void,
): Promise<UnlistenFn> => listen<string>("export:progress", (e) => cb(e.payload));

// ---- transcription ----
export const transcribe = (kind: "url" | "file", value: string) =>
  invoke<void>("transcribe", {
    kind,
    value,
    nowIso: new Date().toISOString(),
  });
export const modelStatus = () => invoke<boolean>("model_status");
export const ytDlpStatus = () => invoke<string | null>("yt_dlp_status");
export const listTranscripts = () =>
  invoke<TranscriptSummary[]>("list_transcripts");
export const openTranscript = (id: string) =>
  invoke<Transcript>("open_transcript", { id });
export const deleteTranscript = (id: string) =>
  invoke<void>("delete_transcript", { id });
export const exportTranscript = (id: string, fmt: string, dest: string) =>
  invoke<void>("export_transcript", { id, fmt, dest });

export const onTranscribeProgress = (
  cb: (p: TranscribeProgress) => void,
): Promise<UnlistenFn> =>
  listen<TranscribeProgress>("transcribe:progress", (e) => cb(e.payload));
export const onTranscribeDone = (
  cb: (t: Transcript) => void,
): Promise<UnlistenFn> =>
  listen<Transcript>("transcribe:done", (e) => cb(e.payload));
export const onTranscribeError = (
  cb: (msg: string) => void,
): Promise<UnlistenFn> =>
  listen<string>("transcribe:error", (e) => cb(e.payload));
