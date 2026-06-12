import { useCallback, useEffect, useState } from "react";
import { listEpisodes, openEpisode, scanSessions } from "../lib/ipc";
import type { SessionSummary } from "../lib/ipc";
import { playSfx } from "../lib/sfx";
import { useKeymap } from "../hooks/useKeymap";
import { Btn } from "../components/Btn";
import type { Session } from "../lib/session";

interface Row {
  dir: string;
  name: string;
  summary?: SessionSummary; // present = resumable
}

// Screen 1 — LOAD: resumable sessions first, then fresh episodes.
// J/K + Enter, rows clickable, R rescans (list refreshes on every entry).
export function Load({
  onOpen,
}: {
  onOpen: (dir: string, s: Session, fresh: boolean) => void;
}) {
  const [rows, setRows] = useState<Row[]>([]);
  const [sel, setSel] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const rescan = useCallback(async () => {
    const [sessions, episodes] = await Promise.all([
      scanSessions(),
      listEpisodes(),
    ]);
    const byDir = new Map(sessions.map((s) => [s.episodeDir, s]));
    const resumable: Row[] = sessions.map((s) => ({
      dir: s.episodeDir,
      name: s.episode,
      summary: s,
    }));
    const fresh: Row[] = episodes
      .filter((dir) => !byDir.has(dir))
      .map((dir) => ({ dir, name: dir.split("/").pop() ?? dir }));
    setRows([...resumable, ...fresh]);
    setSel((s) => Math.min(s, resumable.length + fresh.length - 1));
  }, []);

  useEffect(() => {
    rescan().catch((e) => setError(String(e)));
  }, [rescan]);

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

  useKeymap(
    {
      j: () => {
        setSel((s) => Math.min(s + 1, rows.length - 1));
        playSfx("nav", 0.25);
      },
      arrowdown: () => {
        setSel((s) => Math.min(s + 1, rows.length - 1));
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
      r: () => {
        playSfx("nav", 0.3);
        void rescan().catch((e) => setError(String(e)));
      },
      enter: () => {
        if (rows[sel]) void open(rows[sel]);
      },
    },
    [rows, sel],
  );

  return (
    <div className="screen" style={{ padding: "72px 90px 32px" }}>
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

      <div style={{ overflowY: "auto", flex: 1 }}>
        {rows.map((row, i) => {
          const active = i === sel;
          const s = row.summary;
          return (
            <div
              key={row.dir}
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
          );
        })}
        {rows.length === 0 && !error && (
          <div style={{ color: "var(--faint-cyan)" }}>scanning…</div>
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
        }}
      >
        <span
          style={{
            color: "var(--faint-cyan)",
            fontSize: 10,
            letterSpacing: "0.25em",
          }}
        >
          J/K NAVIGATE · ENTER / CLICK OPEN
        </span>
        <span style={{ flex: 1 }} />
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
