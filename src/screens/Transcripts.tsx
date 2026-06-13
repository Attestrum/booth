import { useCallback, useEffect, useMemo, useState } from "react";
import {
  listTranscripts,
  deleteTranscript,
  revealTranscript,
} from "../lib/ipc";
import type { TranscriptSummary } from "../lib/transcript";
import { playSfx } from "../lib/sfx";
import { useKeymap } from "../hooks/useKeymap";
import { BackButton } from "../components/BackButton";

type Sort = "newest" | "title" | "longest";
const SORTS: Sort[] = ["newest", "title", "longest"];
const SORT_LABEL: Record<Sort, string> = {
  newest: "NEWEST",
  title: "TITLE",
  longest: "LONGEST",
};

type SrcFilter = "all" | "captions" | "whisper";
const SRC_FILTERS: { key: SrcFilter; label: string }[] = [
  { key: "all", label: "ALL" },
  { key: "captions", label: "CAPTIONS" },
  { key: "whisper", label: "WHISPER" },
];

function fmtDur(sec: number) {
  const s = Math.round(sec);
  const p = (n: number) => String(n).padStart(2, "0");
  const h = Math.floor(s / 3600);
  const rest = `${p(Math.floor((s % 3600) / 60))}:${p(s % 60)}`;
  return h > 0 ? `${h}:${rest}` : rest;
}

function fmtDate(iso: string) {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const days = Math.floor((Date.now() - then) / 86_400_000);
  if (days <= 0) return "today";
  if (days === 1) return "1d ago";
  if (days < 30) return `${days}d ago`;
  return new Date(iso).toLocaleDateString();
}

// Screen — the transcript library. Search / sort / filter, open, reveal in
// Finder, delete. Storage is app-data; this is its proper home.
export function Transcripts({
  onBack,
  onOpen,
}: {
  onBack: () => void;
  onOpen: (id: string) => void;
}) {
  const [items, setItems] = useState<TranscriptSummary[] | null>(null);
  const [q, setQ] = useState("");
  const [sort, setSort] = useState<Sort>("newest");
  const [src, setSrc] = useState<SrcFilter>("all");
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(() => {
    listTranscripts()
      .then(setItems)
      .catch((e) => setError(String(e)));
  }, []);
  useEffect(() => refresh(), [refresh]);

  const shown = useMemo(() => {
    let xs = [...(items ?? [])];
    const needle = q.trim().toLowerCase();
    if (needle) {
      xs = xs.filter(
        (t) =>
          t.title.toLowerCase().includes(needle) ||
          t.source.toLowerCase().includes(needle),
      );
    }
    if (src === "whisper") xs = xs.filter((t) => t.segmentSource === "whisper");
    else if (src === "captions")
      xs = xs.filter((t) => t.segmentSource !== "whisper");

    if (sort === "newest") xs.sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    else if (sort === "title") xs.sort((a, b) => a.title.localeCompare(b.title));
    else xs.sort((a, b) => b.durationSec - a.durationSec);
    return xs;
  }, [items, q, sort, src]);

  const remove = async (id: string) => {
    try {
      await deleteTranscript(id);
      playSfx("nav", 0.3);
      setItems((xs) => (xs ?? []).filter((t) => t.id !== id));
    } catch (e) {
      setError(String(e));
    }
  };

  const cycleSort = () => {
    setSort((s) => SORTS[(SORTS.indexOf(s) + 1) % SORTS.length]);
    playSfx("nav", 0.25);
  };

  useKeymap({ escape: onBack, r: refresh, s: cycleSort }, [refresh]);

  const total = items?.length ?? 0;

  return (
    <div className="screen" style={{ padding: "72px 90px 64px" }}>
      <BackButton onClick={onBack} />
      <div
        style={{
          color: "var(--dim-cyan)",
          letterSpacing: "0.42em",
          fontSize: 13,
          marginBottom: 26,
        }}
      >
        ATTESTRUM // BOOTH ▸ TRANSCRIPTS
      </div>

      {/* controls: search · source filter · sort */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 14,
          marginBottom: 22,
          flexWrap: "wrap",
        }}
      >
        <input
          className="url-input"
          style={{ flex: 1, minWidth: 220 }}
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") (e.target as HTMLInputElement).blur();
          }}
          placeholder="search title or source …"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
        />
        <div style={{ display: "flex", gap: 6 }}>
          {SRC_FILTERS.map((f) => (
            <button
              key={f.key}
              type="button"
              className={`chip${src === f.key ? " chip--on" : ""}`}
              onClick={() => setSrc(f.key)}
            >
              {f.label}
            </button>
          ))}
        </div>
        <button type="button" className="chip" onClick={cycleSort}>
          SORT ▸ {SORT_LABEL[sort]}
        </button>
      </div>

      <div style={{ overflowY: "auto", flex: 1 }}>
        {shown.map((t) => (
          <div
            key={t.id}
            className="load-row"
            data-autopilot={`lib-${t.id}`}
            onClick={() => {
              playSfx("toggle");
              onOpen(t.id);
            }}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 22,
              padding: "14px 18px",
              marginBottom: 6,
              border: "1px solid var(--faint-cyan)",
              color: "var(--dim-cyan)",
            }}
          >
            <span style={{ fontSize: 11, letterSpacing: "0.2em", width: 64 }}>
              OPEN
            </span>
            <span style={{ flex: 1, letterSpacing: "0.06em", fontSize: 15 }}>
              {t.title}
            </span>
            <span style={{ fontSize: 11, textAlign: "right", whiteSpace: "nowrap" }}>
              {t.segmentSource} · {fmtDur(t.durationSec)} · {t.nSegments} seg ·{" "}
              {fmtDate(t.createdAt)}
            </span>
            <button
              type="button"
              className="row-action"
              title="Reveal the .json in Finder"
              onClick={(e) => {
                e.stopPropagation();
                void revealTranscript(t.id).catch((err) => setError(String(err)));
              }}
            >
              ⤓
            </button>
            <button
              type="button"
              className="take-delete"
              title="Delete transcript"
              onClick={(e) => {
                e.stopPropagation();
                void remove(t.id);
              }}
            >
              ✕
            </button>
          </div>
        ))}

        {items === null && !error && (
          <div style={{ color: "var(--dim-cyan)" }}>loading…</div>
        )}
        {items !== null && total === 0 && !error && (
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              gap: 14,
              marginTop: 90,
              color: "var(--dim-cyan)",
            }}
          >
            <div style={{ fontSize: 13, letterSpacing: "0.2em" }}>
              NO TRANSCRIPTS YET
            </div>
            <div style={{ fontSize: 12, lineHeight: 1.6, maxWidth: 460, textAlign: "center" }}>
              Paste a link or pick a file on the previous screen to transcribe —
              results land here.
            </div>
          </div>
        )}
        {items !== null && total > 0 && shown.length === 0 && (
          <div style={{ color: "var(--dim-cyan)", marginTop: 20 }}>
            no transcripts match “{q}”.
          </div>
        )}
      </div>

      {error && (
        <div style={{ color: "var(--amber)", fontSize: 12, marginTop: 16 }}>
          ⚠ {error}
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
          style={{ color: "var(--dim-cyan)", fontSize: 10, letterSpacing: "0.25em" }}
        >
          ESC ▸ BACK · S SORT · R RESCAN
        </span>
        <span style={{ flex: 1 }} />
        <span
          style={{ color: "var(--dim-cyan)", fontSize: 11, letterSpacing: "0.15em" }}
        >
          {shown.length}
          {shown.length !== total ? ` / ${total}` : ""} TRANSCRIPT
          {total === 1 ? "" : "S"}
        </span>
      </div>
    </div>
  );
}
