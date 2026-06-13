import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { downloadDir } from "@tauri-apps/api/path";
import {
  addProject,
  deleteTranscript,
  getRecents,
  importScript,
  listEpisodes,
  listTranscripts,
  openEpisode,
  scanSessions,
} from "../lib/ipc";
import type { SessionSummary } from "../lib/ipc";
import type { TranscriptSummary } from "../lib/transcript";
import { playSfx } from "../lib/sfx";
import { useKeymap } from "../hooks/useKeymap";
import { Btn } from "../components/Btn";
import type { Session } from "../lib/session";

const MEDIA_EXTS = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "mp4", "mov", "m4v"];

interface Row {
  dir: string;
  name: string;
  root: string; // the recent project folder this row was found under
  summary?: SessionSummary; // present = resumable
}

// Screen 1 — LOAD: recent project folders, resumable sessions first within
// each. O opens a new folder, J/K + Enter, rows clickable, R rescans.
// First run shows an empty state with OPEN FOLDER.
export function Load({
  onOpen,
  onTranscribe,
  onOpenTranscript,
}: {
  onOpen: (dir: string, s: Session, fresh: boolean) => void;
  onTranscribe: (kind: "url" | "file", value: string) => void;
  onOpenTranscript: (id: string) => void;
}) {
  const [rows, setRows] = useState<Row[] | null>(null); // null = scanning
  const [sel, setSel] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [url, setUrl] = useState("");
  const [transcripts, setTranscripts] = useState<TranscriptSummary[]>([]);

  const rescan = useCallback(async () => {
    const roots = await getRecents();
    const all: Row[] = [];
    for (const root of roots) {
      const [sessions, episodes] = await Promise.all([
        scanSessions(root),
        listEpisodes(root),
      ]);
      const byDir = new Set(sessions.map((s) => s.episodeDir));
      all.push(
        ...sessions.map((s) => ({
          dir: s.episodeDir,
          name: s.episode,
          root,
          summary: s,
        })),
        ...episodes
          .filter((dir) => !byDir.has(dir))
          .map((dir) => ({
            dir,
            name: dir.split("/").pop() ?? dir,
            root,
          })),
      );
    }
    setRows(all);
    setSel((s) => Math.max(0, Math.min(s, all.length - 1)));
    listTranscripts()
      .then(setTranscripts)
      .catch(() => {});
  }, []);

  useEffect(() => {
    rescan().catch((e) => setError(String(e)));
  }, [rescan]);

  const openFolder = async () => {
    try {
      const dir = await openDialog({
        directory: true,
        title: "Open a project folder (your script's folder, or a folder of episodes)",
      });
      if (typeof dir !== "string") return;
      await addProject(dir);
      playSfx("toggle", 0.4);
      await rescan();
    } catch (e) {
      playSfx("error");
      setError(String(e));
    }
  };

  const open = async (row: Row) => {
    try {
      const { session, fresh } = await openEpisode(row.dir);
      playSfx("toggle");
      onOpen(row.dir, session, fresh);
    } catch (e) {
      playSfx("error");
      setError(String(e));
    }
  };

  const submitUrl = () => {
    const u = url.trim();
    if (!u) return;
    playSfx("toggle");
    onTranscribe("url", u);
  };

  const pickMediaFile = async () => {
    try {
      const file = await openDialog({
        title: "Choose an audio or video file to transcribe",
        defaultPath: await downloadDir().catch(() => undefined),
        filters: [{ name: "Audio / Video", extensions: MEDIA_EXTS }],
      });
      if (typeof file !== "string") return;
      playSfx("toggle");
      onTranscribe("file", file);
    } catch (e) {
      playSfx("error");
      setError(String(e));
    }
  };

  const removeTranscript = async (id: string) => {
    try {
      await deleteTranscript(id);
      playSfx("nav", 0.3);
      setTranscripts((ts) => ts.filter((t) => t.id !== id));
    } catch (e) {
      setError(String(e));
    }
  };

  const importFile = async () => {
    try {
      const file = await openDialog({
        title: "Import a script (.md / .txt)",
        filters: [{ name: "Scripts", extensions: ["md", "markdown", "txt"] }],
      });
      if (typeof file !== "string") return;
      const { dir, session, fresh } = await importScript(file);
      playSfx("toggle");
      onOpen(dir, session, fresh);
    } catch (e) {
      playSfx("error");
      setError(String(e));
    }
  };

  const list = rows ?? [];

  useKeymap(
    {
      j: () => {
        setSel((s) => Math.min(s + 1, list.length - 1));
        playSfx("nav", 0.25);
      },
      arrowdown: () => {
        setSel((s) => Math.min(s + 1, list.length - 1));
        playSfx("nav", 0.25);
      },
      k: () => {
        setSel((s) => Math.max(s - 1, 0));
        playSfx("nav", 0.25);
      },
      arrowup: () => {
        setSel((s) => Math.max(s - 1, 0));
        playSfx("nav", 0.25);
      },
      o: () => void openFolder(),
      i: () => void importFile(),
      f: () => void pickMediaFile(),
      r: () => {
        playSfx("nav", 0.3);
        void rescan().catch((e) => setError(String(e)));
      },
      enter: () => {
        if (list[sel]) void open(list[sel]);
      },
    },
    [rows, sel],
  );

  const shortRoot = (root: string) =>
    root.replace(/^\/Users\/[^/]+/, "~").replace(/^\/home\/[^/]+/, "~");

  return (
    <div className="screen" style={{ padding: "72px 90px 64px" }}>
      <div
        style={{
          color: "var(--dim-cyan)",
          letterSpacing: "0.42em",
          fontSize: 13,
          marginBottom: 36,
        }}
      >
        ATTESTRUM // BOOTH ▸ SELECT TRANSMISSION
      </div>

      <div className="tx-bar">
        <span className="tx-bar-label">TRANSCRIBE ▸</span>
        <input
          className="url-input"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submitUrl();
            else if (e.key === "Escape") (e.target as HTMLInputElement).blur();
          }}
          placeholder="paste a youtube / tiktok / video link …"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
        />
        <Btn id="tx-file" label="⌁ File" hint="f" onClick={() => void pickMediaFile()} />
        <Btn
          id="tx-go"
          label="Transcribe"
          variant="success"
          disabled={!url.trim()}
          onClick={submitUrl}
        />
      </div>

      <div style={{ overflowY: "auto", flex: 1 }}>
        {list.map((row, i) => {
          const active = i === sel;
          const s = row.summary;
          const newRoot = i === 0 || list[i - 1].root !== row.root;
          return (
            <div key={row.dir}>
              {newRoot && (
                <div
                  style={{
                    color: "var(--dim-cyan)",
                    fontSize: 10,
                    letterSpacing: "0.25em",
                    margin: "14px 0 8px",
                    borderBottom: "1px solid var(--faint-cyan)",
                    paddingBottom: 4,
                  }}
                >
                  {shortRoot(row.root).toUpperCase()}
                </div>
              )}
              <div
                className="load-row"
                data-autopilot={`row-${i}`}
                onClick={() => {
                  setSel(i);
                  void open(row);
                }}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 24,
                  padding: "14px 18px",
                  marginBottom: 6,
                  border: `1px solid ${active ? "var(--dim-cyan)" : "var(--faint-cyan)"}`,
                  background: active ? "var(--faint-cyan)" : "transparent",
                  color: active ? "var(--cyan)" : "var(--dim-cyan)",
                  textShadow: active ? "0 0 12px var(--dim-cyan)" : "none",
                }}
              >
                <span style={{ fontSize: 11, letterSpacing: "0.2em", width: 70 }}>
                  {s ? "RESUME" : "NEW"}
                </span>
                <span style={{ flex: 1, letterSpacing: "0.08em", fontSize: 15 }}>
                  {row.name}
                </span>
                {s && (
                  <>
                    <Progress done={s.recorded} total={s.total} />
                    <span style={{ fontSize: 11, width: 110, textAlign: "right" }}>
                      {s.recorded}/{s.total} · {s.takes} takes
                    </span>
                  </>
                )}
                <span className="load-row-open" style={{ fontSize: 11 }}>
                  OPEN ▸
                </span>
              </div>
            </div>
          );
        })}

        {transcripts.length > 0 && (
          <>
            <div
              style={{
                color: "var(--dim-cyan)",
                fontSize: 10,
                letterSpacing: "0.25em",
                margin: "18px 0 8px",
                borderBottom: "1px solid var(--faint-cyan)",
                paddingBottom: 4,
              }}
            >
              ◢ TRANSCRIPTS
            </div>
            {transcripts.map((t) => (
              <div
                key={t.id}
                className="load-row"
                data-autopilot={`transcript-${t.id}`}
                onClick={() => {
                  playSfx("toggle");
                  onOpenTranscript(t.id);
                }}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 24,
                  padding: "14px 18px",
                  marginBottom: 6,
                  border: "1px solid var(--faint-cyan)",
                  color: "var(--dim-cyan)",
                }}
              >
                <span style={{ fontSize: 11, letterSpacing: "0.2em", width: 70 }}>
                  OPEN
                </span>
                <span style={{ flex: 1, letterSpacing: "0.08em", fontSize: 15 }}>
                  {t.title}
                </span>
                <span style={{ fontSize: 11, textAlign: "right" }}>
                  {t.segmentSource} · {fmtDur(t.durationSec)} · {t.nSegments} seg
                </span>
                <button
                  className="take-delete"
                  title="Delete transcript"
                  onClick={(e) => {
                    e.stopPropagation();
                    void removeTranscript(t.id);
                  }}
                >
                  ✕
                </button>
              </div>
            ))}
          </>
        )}

        {rows === null && !error && (
          <div style={{ color: "var(--dim-cyan)" }}>scanning…</div>
        )}
        {rows !== null && list.length === 0 && !error && (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              gap: 18,
              marginTop: 80,
            }}
          >
            <div
              style={{
                color: "var(--dim-cyan)",
                fontSize: 13,
                letterSpacing: "0.2em",
              }}
            >
              NO PROJECTS YET
            </div>
            <div
              style={{
                color: "var(--dim-cyan)",
                fontSize: 12,
                lineHeight: 1.6,
                maxWidth: 460,
                textAlign: "center",
              }}
            >
              Import a script (.md / .txt) to start recording, or open a folder
              that already holds booth sessions. Booth keeps its sessions and
              exports next to your script.
            </div>
            <div style={{ display: "flex", gap: 10 }}>
              <Btn
                id="import-script-empty"
                label="Import Script…"
                hint="i"
                variant="success"
                onClick={() => void importFile()}
              />
              <Btn id="open-folder-empty" label="Open Folder…" hint="o" onClick={() => void openFolder()} />
            </div>
          </div>
        )}
      </div>

      {error && (
        <div style={{ color: "var(--amber)", fontSize: 12, marginTop: 16 }}>
          {error}
        </div>
      )}

      <div
        style={{
          display: "flex",
          alignItems: "center",
          marginTop: 20,
          gap: 10,
        }}
      >
        <span
          style={{
            color: "var(--dim-cyan)",
            fontSize: 10,
            letterSpacing: "0.25em",
          }}
        >
          J/K NAVIGATE · ENTER / CLICK OPEN
        </span>
        <span style={{ flex: 1 }} />
        <Btn
          id="import-script"
          label="Import Script…"
          hint="i"
          onClick={() => void importFile()}
        />
        <Btn id="open-folder" label="Open Folder…" hint="o" onClick={() => void openFolder()} />
        <Btn
          id="rescan"
          label="Rescan"
          hint="r"
          onClick={() => {
            playSfx("nav", 0.3);
            void rescan().catch((e) => setError(String(e)));
          }}
        />
      </div>
    </div>
  );
}

function fmtDur(sec: number) {
  const s = Math.round(sec);
  const p = (n: number) => String(n).padStart(2, "0");
  const h = Math.floor(s / 3600);
  const rest = `${p(Math.floor((s % 3600) / 60))}:${p(s % 60)}`;
  return h > 0 ? `${h}:${rest}` : rest;
}

function Progress({ done, total }: { done: number; total: number }) {
  return (
    <div
      style={{
        width: 160,
        height: 4,
        background: "var(--faint-cyan)",
        position: "relative",
      }}
    >
      <div
        style={{
          position: "absolute",
          inset: 0,
          width: `${total ? (done / total) * 100 : 0}%`,
          background: "var(--cyan)",
          boxShadow: "0 0 8px var(--dim-cyan)",
        }}
      />
    </div>
  );
}
