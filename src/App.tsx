import { useState, useCallback } from "react";
import { useAutopilot } from "./hooks/useAutopilot";
import { saveSession } from "./lib/ipc";
import { Scanlines } from "./components/Scanlines";
import { HelpOverlay } from "./components/HelpOverlay";
import { PowerOn } from "./screens/PowerOn";
import { Load } from "./screens/Load";
import { Grouping } from "./screens/Grouping";
import { Booth } from "./screens/Booth";
import { Review } from "./screens/Review";
import { Transcribe, type TranscribeArg } from "./screens/Transcribe";
import { Transcripts } from "./screens/Transcripts";
import type { Session } from "./lib/session";

export type Screen =
  | "power-on"
  | "load"
  | "grouping"
  | "booth"
  | "review"
  | "transcribe"
  | "transcripts";

export default function App() {
  useAutopilot();
  const [screen, setScreen] = useState<Screen>("power-on");
  // absolute path to the active episode folder + its loaded session
  const [episodeDir, setEpisodeDir] = useState<string | null>(null);
  const [session, setSession] = useState<Session | null>(null);
  // what the Transcribe screen is showing (a fresh job or a saved transcript)
  const [txArg, setTxArg] = useState<TranscribeArg | null>(null);

  const openSession = useCallback((dir: string, s: Session, fresh: boolean) => {
    setEpisodeDir(dir);
    setSession(s);
    setScreen(fresh ? "grouping" : "booth");
  }, []);

  const startTranscribe = useCallback((arg: TranscribeArg) => {
    setTxArg(arg);
    setScreen("transcribe");
  }, []);

  return (
    <>
      <div className="titlebar-drag" />
      {screen === "power-on" && <PowerOn onDone={() => setScreen("load")} />}
      {screen === "load" && (
        <Load
          onOpen={openSession}
          onTranscribe={(kind, value) =>
            startTranscribe({ mode: "run", kind, value })
          }
          onShowTranscripts={() => setScreen("transcripts")}
        />
      )}
      {screen === "transcripts" && (
        <Transcripts
          onBack={() => setScreen("load")}
          onOpen={(id) => startTranscribe({ mode: "open", id })}
        />
      )}
      {screen === "grouping" && episodeDir && session && (
        <Grouping
          episodeDir={episodeDir}
          session={session}
          onSession={setSession}
          onConfirm={() => setScreen("booth")}
          onBack={() => setScreen("load")}
        />
      )}
      {screen === "booth" && episodeDir && session && (
        <Booth
          episodeDir={episodeDir}
          session={session}
          onSession={setSession}
          onRegroup={() => setScreen("grouping")}
          onReview={() => setScreen("review")}
          onBack={() => setScreen("load")}
        />
      )}
      {screen === "review" && episodeDir && session && (
        <Review
          episodeDir={episodeDir}
          session={session}
          onBack={() => setScreen("booth")}
          onJump={(passage) => {
            const s = { ...session, cursor: passage };
            setSession(s);
            void saveSession(episodeDir, s);
            setScreen("booth");
          }}
        />
      )}
      {screen === "transcribe" && txArg && (
        <Transcribe arg={txArg} onBack={() => setScreen("load")} />
      )}
      {screen !== "power-on" && <HelpOverlay />}
      <Scanlines />
    </>
  );
}
