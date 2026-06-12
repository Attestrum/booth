import { useEffect, useRef, useState } from "react";
import type { Session } from "../lib/session";
import { passageText } from "../lib/session";
import { playSfx } from "../lib/sfx";

// Current passage large + glowing, prev/next as dim ghosts, amber cue footnote.
// On passage change the text prints in fast (~300 ms whole passage) with one
// transmission tick — single pass, then it sits.
//
// Fit rules (founder 2026-06-12 — long passages were overflowing under the top
// rail and clipping the cue): font auto-scales with passage length, centering
// is overflow-safe (scrolls instead of clipping both ends), ghosts hide when
// the passage is long, and the cue line never shrinks away.
//
// Inline edit (founder 2026-06-12): click the passage text (idle only) to edit
// it in place — one paragraph per unit. SAVE button (greyed until the text
// changes) or ⌘S saves (propagates to script-units.json + completed-videos
// script.md via edit_unit_text); Esc OR clicking anywhere outside cancels.
export function Teleprompter({
  session,
  cursor,
  editable,
  onSaveEdits,
}: {
  session: Session;
  cursor: number;
  editable: boolean;
  onSaveEdits: (updates: { unit: number; text: string }[]) => Promise<void>;
}) {
  const prev = session.passages[cursor - 1];
  const cur = session.passages[cursor];
  const next = session.passages[cursor + 1];
  const cue = cur ? session.units[cur.unitStart]?.cue : "";
  // cursor is always clamped to a valid passage (DESIGN.md §6 invariants)
  const fullText = cur ? passageText(session, cur) : "";
  const unitTexts = cur
    ? session.units.slice(cur.unitStart, cur.unitEnd + 1).map((u) => u.text)
    : [];

  // 26px up to ~300 chars, easing down to 18px by ~650 chars
  const fontSize = Math.round(
    Math.max(18, Math.min(26, 26 - (fullText.length - 300) / 45)),
  );
  const showGhosts = fullText.length <= 420;

  const [shown, setShown] = useState(fullText);
  const lastCursor = useRef(cursor);
  useEffect(() => {
    if (cursor === lastCursor.current) {
      setShown(fullText);
      return;
    }
    lastCursor.current = cursor;
    playSfx("tick", 0.3);
    const start = performance.now();
    let raf = 0;
    const step = (t: number) => {
      const p = Math.min(1, (t - start) / 300);
      setShown(fullText.slice(0, Math.ceil(fullText.length * p)));
      if (p < 1) raf = requestAnimationFrame(step);
    };
    raf = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf);
  }, [cursor, fullText]);

  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [editErr, setEditErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // click-outside with unsaved changes asks before discarding
  const [confirmExit, setConfirmExit] = useState(false);
  const dirty = editing && draft !== unitTexts.join("\n\n");
  useEffect(() => {
    // navigating away abandons an open edit
    setEditing(false);
    setEditErr(null);
    setConfirmExit(false);
  }, [cursor]);

  const startEdit = () => {
    if (!editable || !cur || editing) return;
    setDraft(unitTexts.join("\n\n"));
    setEditErr(null);
    setConfirmExit(false);
    setEditing(true);
    playSfx("nav", 0.25);
  };

  const cancelEdit = () => {
    setEditing(false);
    setConfirmExit(false);
    setEditErr(null);
  };

  const saveEdit = async () => {
    if (saving) return;
    setConfirmExit(false);
    const parts = draft
      .split(/\n\s*\n/)
      .map((s) => s.trim())
      .filter(Boolean);
    if (parts.length !== unitTexts.length) {
      setEditErr(
        `keep ${unitTexts.length} paragraph${unitTexts.length === 1 ? "" : "s"} — one per unit (blank line between)`,
      );
      return;
    }
    const updates = parts
      .map((text, i) => ({ unit: cur.unitStart + i, text }))
      .filter((u, i) => u.text !== unitTexts[i]);
    if (updates.length === 0) {
      setEditing(false);
      return;
    }
    setSaving(true);
    try {
      await onSaveEdits(updates);
      setEditing(false);
    } catch (e) {
      setEditErr(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      style={{
        flex: 1,
        minHeight: 0, // allow the flex child to actually shrink + scroll
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        gap: 22,
        overflowY: "auto",
        padding: "16px 24px",
      }}
    >
      {/* margin auto on first/last children = overflow-safe centering:
          content taller than the box scrolls instead of clipping both ends */}
      <div style={{ marginTop: "auto" }} />
      {!editing && showGhosts && prev && (
        <div
          style={{
            color: "var(--dim-cyan)",
            fontSize: 13,
            lineHeight: 1.5,
            maxHeight: 60,
            overflow: "hidden",
            flexShrink: 0,
            maskImage: "linear-gradient(to top, black 30%, transparent)",
            WebkitMaskImage: "linear-gradient(to top, black 30%, transparent)",
          }}
        >
          {passageText(session, prev)}
        </div>
      )}
      {editing ? (
        <div style={{ flexShrink: 0 }}>
          <textarea
            className="prompter-edit"
            autoFocus
            value={draft}
            rows={Math.max(4, draft.split("\n").length + 2)}
            style={{ fontSize, lineHeight: 1.65 }}
            onChange={(e) => setDraft(e.target.value)}
            onFocus={() => setConfirmExit(false)}
            onBlur={() => {
              // clicking anywhere outside: clean draft exits silently, a
              // dirty one asks first (SAVE/CANCEL keep focus via their
              // mousedown preventDefault, so they never land here)
              if (saving) return;
              if (!dirty) setEditing(false);
              else setConfirmExit(true);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                cancelEdit();
              } else if (e.metaKey && e.key.toLowerCase() === "s") {
                e.preventDefault();
                void saveEdit();
              }
            }}
          />
          {confirmExit ? (
            <div
              style={{
                display: "flex",
                gap: 14,
                alignItems: "center",
                marginTop: 8,
                fontSize: 11,
                letterSpacing: "0.15em",
                color: "var(--amber)",
              }}
              onKeyDown={(e) => {
                e.stopPropagation(); // keep booth keys out of the prompt
                if (e.key === "Enter") void saveEdit();
                if (e.key === "Escape") cancelEdit();
              }}
            >
              <span>⚠ SAVE CHANGES?</span>
              <button
                type="button"
                className="btn btn--success"
                data-autopilot="confirm-save"
                autoFocus
                onClick={() => void saveEdit()}
              >
                Save<span className="btn-hint">⏎</span>
              </button>
              <button
                type="button"
                className="btn btn--danger"
                data-autopilot="confirm-discard"
                onClick={cancelEdit}
              >
                Discard<span className="btn-hint">esc</span>
              </button>
            </div>
          ) : (
            <div
              style={{
                display: "flex",
                gap: 14,
                alignItems: "center",
                marginTop: 8,
                fontSize: 10,
                letterSpacing: "0.2em",
                color: "var(--dim-cyan)",
              }}
            >
              <button
                type="button"
                className="btn btn--success"
                data-autopilot="save-edit"
                disabled={!dirty || saving}
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => void saveEdit()}
              >
                {saving ? "Saving…" : "Save"}
                <span className="btn-hint">⌘s</span>
              </button>
              <button
                type="button"
                className="btn"
                data-autopilot="cancel-edit"
                onMouseDown={(e) => e.preventDefault()}
                onClick={cancelEdit}
              >
                Cancel<span className="btn-hint">esc</span>
              </button>
              <span>
                {`${unitTexts.length} PARAGRAPH${unitTexts.length === 1 ? "" : "S"} = ${unitTexts.length} UNIT${unitTexts.length === 1 ? "" : "S"}`}
              </span>
              {editErr && (
                <span style={{ color: "var(--amber)", letterSpacing: "0.1em" }}>
                  ⚠ {editErr}
                </span>
              )}
            </div>
          )}
        </div>
      ) : (
        <div
          className={editable ? "prompter-text prompter-text--editable" : "prompter-text"}
          title={editable ? "Click to edit this passage" : undefined}
          onClick={startEdit}
          style={{
            color: "var(--cyan)",
            fontSize,
            lineHeight: 1.65,
            textShadow: "0 0 12px var(--dim-cyan)",
            letterSpacing: "0.01em",
            flexShrink: 0,
          }}
        >
          {shown}
        </div>
      )}
      {!editing && cue && (
        <div
          style={{
            color: "var(--amber)",
            fontSize: 12,
            opacity: 0.8,
            flexShrink: 0,
            lineHeight: 1.5,
          }}
        >
          {cue}
        </div>
      )}
      {!editing && showGhosts && next && (
        <div
          style={{
            color: "var(--dim-cyan)",
            fontSize: 13,
            lineHeight: 1.5,
            maxHeight: 60,
            overflow: "hidden",
            flexShrink: 0,
            maskImage: "linear-gradient(to bottom, black 30%, transparent)",
            WebkitMaskImage:
              "linear-gradient(to bottom, black 30%, transparent)",
          }}
        >
          {passageText(session, next)}
        </div>
      )}
      <div style={{ marginBottom: "auto" }} />
    </div>
  );
}
