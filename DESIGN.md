# BOOTH — design spec (living document)

**Mandatory reading before changing booth behavior.** Every screen, state, and side effect is
diagrammed here; if a change adds a transition, it MUST be added to the matching diagram first —
the missing-ESC bug existed precisely because exits were never enumerated. Keep diagrams and code
in lockstep.

Aesthetic + motion law come from the workspace brains: cyan `#7FE0FF` on `#0A0E14`, IBM Plex Mono,
animations are perfect seamless loops OR single-pass→settle (VIDEO-BRAIN §1). No scanlines
(founder, 2026-06-12).

---

## 1. Screen navigation

Every node lists every exit. Buttons and keys are the same transitions (buttons carry key hints).

```mermaid
flowchart TD
    PO["POWER-ON\n(press any key / click)"] -->|"1.5s CRT sequence"| LOAD

    LOAD["LOAD — recent project folders,\nresumable sessions first per folder\n(first run: empty state + OPEN FOLDER)"]
    LOAD -->|"O / OPEN FOLDER…\n(native dialog → recents)"| LOAD
    LOAD -->|"I / IMPORT SCRIPT…\n(.md/.txt → units → fresh session)"| GROUP
    LOAD -->|"Enter / click row\n(no session.json → fresh)"| GROUP
    LOAD -->|"Enter / click row\n(session.json exists → resume)"| BOOTH
    LOAD -->|"R / RESCAN button"| LOAD
    LOAD -->|"J/K move selection"| LOAD

    GROUP["TRANSCRIPT — full segment list,\nmerge/split passages"]
    GROUP -->|"Enter / BEGIN ▸ BOOTH\n(cursor = SELECTED row)"| BOOTH
    GROUP -->|"Esc / ‹ EPISODES"| LOAD
    GROUP -->|"J/K · M merge · S split\n(locked if passage has takes)"| GROUP

    BOOTH["BOOTH — record loop"]
    BOOTH -->|"G / REGROUP (idle only)"| GROUP
    BOOTH -->|"Tab / REVIEW (idle only)"| REVIEW
    BOOTH -->|"Esc / ‹ EPISODES (idle only)"| LOAD
    BOOTH -->|"completion chip click\n(all passages recorded)"| REVIEW

    REVIEW["REVIEW — ledger + export"]
    REVIEW -->|"Tab · Esc / ‹ BOOTH"| BOOTH
    REVIEW -->|"Enter / click row\n(jump to that passage)"| BOOTH
    REVIEW -->|"⌘E / EXPORT (all recorded)\n⇧⌘E / EXPORT PARTIAL"| REVIEW
```

**Contract:** LOAD rescans on every entry. BOOTH blocks ALL navigation while recording (the only
live exit is STOP). REVIEW row-jump sets `session.cursor` before switching. Cmd+Q (native) is
always available; a quit mid-take is recovered on next launch (diagram 4).

**Project model (replaced the hardcoded episodes root, 2026-06-12):** LOAD lists sessions found
under the user's **recent project folders** (persisted as `config.json` in the OS app-config dir,
newest first, capped at 8 — `src-tauri/src/config.rs`). A project folder is either a single
script's folder or a folder whose immediate subfolders are episodes; `session::scan` checks the
root itself plus one level down, and `session::list_candidates` lists fresh openables (folders
with a parseable `narration/` script but no session). Asset-protocol access for take playback is
granted **per opened folder at runtime** (`asset_protocol_scope().allow_directory`) — the static
scope in `tauri.conf.json` is empty by design.

**Script import (.md/.txt):** `import_script` parses the document
(`script::units_from_document` — blank-line paragraphs sentence-split into units; markdown
headings become chapters, fenced code skipped, light inline strip; a `[VISUAL:`/`[CUE:`
paragraph becomes the preceding unit's cue) and persists the result as the folder's
`narration/script-units.json`, so the standard parse ladder and every downstream feature work
unchanged. `session.sourceFile` links back to the imported document for inline-edit write-back.
Re-importing into a folder that already has a session is refused (open or remove the session
first). Unsupported extensions are rejected naming .md/.txt.

## 2. Booth interaction states

```mermaid
stateDiagram-v2
    [*] --> Idle

    Idle --> Recording : Space / ● REC\n(busy-guard serialises invoke)
    Recording --> Idle : Space / ■ STOP\n(take pushed, ping sfx)
    Recording --> Idle : start/stop error\n(amber error chip)

    Idle --> Playing : P / ▶ PLAY (top take exists)
    Playing --> Idle : P again · playback ends ·\nnav · record · accept · Esc

    Idle --> RevertArmed : R / ↩ REVERT (takes > 0)\ntop card turns amber
    RevertArmed --> Idle : 600 ms timeout
    RevertArmed --> Idle : nav / record / accept / Esc\n(DISARMS — gap fix #1)
    RevertArmed --> UndoWindow : R again / CONFIRM ↩\n(take → discarded/)

    Idle --> UndoWindow : ✕ on ANY take card (single click)\n(that take → discarded/)

    UndoWindow --> Idle : 5 s timeout (chip fades)
    UndoWindow --> Idle : U / ↶ UNDO\n(take restored to top)
    UndoWindow --> Idle : new take recorded on that passage\n(CANCELS undo — gap fix #2)

    note right of Recording
        nav J/K, G, Tab, Esc all BLOCKED
        UI sfx hard-muted (no mic bleed)
        >2 s with peak<0.001 → NO SIGNAL chip (gap fix #4)
    end note
```

**Contract:** `revertArmed` and the undo window are per-passage intents — ANY action that changes
context (navigate, record, accept, leave screen) disarms/cancels them. Undo never reorders a
stack that has changed since the revert.

**Per-take delete (✕, founder 2026-06-12):** every take card carries a ✕ that fires on a SINGLE
click — no confirm; the 5 s undo window is the safety net (founder's call). Disk semantics are
identical to revert — the file MOVES to `discarded/`, never deleted. Deleting the top take clears
`accepted` (same as revert); deleting a lower take leaves the accepted top take alone. Caveat:
undoing a mid-stack delete restores the take to the TOP of the stack, not its original position.

**Control bar width is constant:** the ↶ UNDO button renders next to the amber TAKE DISCARDED
status line, NOT in the control bar — a conditional button in the bar widened it past the window
edge and clipped REVIEW (gap #13).

## 3. Recording data flow

```mermaid
sequenceDiagram
    participant UI as Booth.tsx
    participant IPC as lib.rs commands
    participant ENG as AudioEngine thread
    participant CB as CoreAudio callback
    participant FS as takes/pNNN_tNN.wav

    UI->>UI: toggle sfx BEFORE stream opens, gate SFX
    UI->>IPC: start_recording(dir, passage)
    IPC->>IPC: session::load (any device rate accepted)
    IPC->>ENG: Cmd::Start{path}
    ENG->>CB: build_input_stream (f32/i16 → mono f32)
    loop every audio buffer
        CB->>FS: 24-bit samples (hound writer)
        CB->>ENG: pending samples (shared ring)
    end
    loop every 33 ms (30 Hz)
        ENG->>FS: writer.flush() — header stays valid (crash safety)
        ENG-->>UI: audio:frame {rms, peak, clip, window[128]}
    end
    UI->>IPC: stop_recording()
    IPC->>ENG: Cmd::Stop → drop stream, finalize writer
    IPC->>IPC: push Take, pin format on first take, atomic save
    IPC-->>UI: updated Session (UI re-renders stack, ungates SFX)
```

**Contract:** the webview NEVER touches the mic. A UI crash/reload loses at most 33 ms of header
freshness, never samples. Mixed sample rates within a session are ALLOWED (rate gate removed,
founder 2026-06-12) — each take's WAV carries its own rate; export normalizes before concat
(diagram 5). `session.format` is informational only: it tracks the LATEST take for the top rail.

## 4. Session & disk lifecycle

```mermaid
flowchart TD
    OPEN["open_episode(dir)"] --> HAS{"booth/session.json?"}
    HAS -->|yes| REC["RIFF recovery pass:\npatch stale headers from crash,\nflag takes [RECOVERED]"] --> RESUME["resume (cursor, stacks intact)"]
    HAS -->|no| LADDER{"parse ladder"}
    LADDER -->|"narration/script-units.json"| GROUPDEF
    LADDER -->|"narration/chunks.json\n(sentence-split)"| GROUPDEF
    LADDER -->|neither| ERR["amber error on LOAD"]
    GROUPDEF["default grouping:\ngreedy ≤600 chars,\nchapter barriers"] --> FRESH["fresh session → GROUPING"]

    subgraph disk ["narration/booth/ (inside the episode — travels with it)"]
        SJ["session.json\natomic temp+rename on EVERY change"]
        TK["takes/p013_t02.wav\nflushed @30 Hz"]
        DC["discarded/ — reverts move here,\nNOTHING deleted until export"]
        RP["replaced/ — timestamped backups\nof prior voice.wav/.mp3"]
    end
```

**Contract:** Rust is stateless between commands (load→mutate→save); a crash can lose at most the
operation in flight. Take filenames scan BOTH takes/ and discarded/ for the next number — a
discarded `p001_t02` is never reused.

## 5. Export pipeline

```mermaid
flowchart TD
    E["⌘E EXPORT"] --> MISS{"passages without takes?"}
    MISS -->|"yes + no override"| REFUSE["amber: 'N unrecorded — ⇧⌘E for partial'"]
    MISS -->|"none, or ⇧⌘E"| BAK["backup existing voice.wav/.mp3\n→ booth/replaced/&lt;name&gt;.&lt;unix-ts&gt;.bak\n(timestamped — never clobbers, gap fix #3)"]
    BAK --> NORM{"takes share one rate?"}
    NORM -->|yes| CAT["pure-Rust WAV concat of TOP takes\nin passage order + 350 ms gaps\n(sample-exact, format-checked)"]
    NORM -->|"no (mixed devices)"| RS["ffmpeg resample minority takes\n→ highest take rate, temp dir"] --> CAT
    CAT --> FF{"ffmpeg installed?"}
    FF -->|yes| ENC["ffmpeg → voice.mp3 44.1 kHz mono 192k\n(/opt/homebrew/bin/ffmpeg, which-fallback)"]
    FF -->|"no (wav-only export)"| SEAL
    ENC --> SEAL["✓ TRANSMISSION SEALED\n(chime + glitch, paths printed)"]
    CAT -->|format mismatch / IO error| FAIL["amber error chip"]
    RS -->|"ffmpeg missing (clear error\nnaming the mixed rates) / fails"| FAIL
    ENC -->|ffmpeg fails| FAIL
```

**Contract:** export reads top-of-stack only; discarded takes never ship. Output satisfies the
pipeline contract consumed by `tools/align.sh` and `tools/sync-to-vo.py` unchanged.

## 6. session.json schema

```mermaid
classDiagram
    class Session {
        schema: 1
        episode: string
        source: "script-units.json" | "chunks.json"
        format: AudioFormat | null  // latest take's format (display only)
        units: ScriptUnit[]         // snapshot; text editable in-booth —
                                    // edits propagate to script-units.json
                                    // + the linked sourceFile document
        passages: Passage[]
        cursor: number
        createdAt: ISO string
        device: string | null
        sourceFile: string | null   // imported document (write-back target;
                                    // absent on pre-project-model sessions)
    }
    class Passage {
        unitStart: number  // inclusive
        unitEnd: number    // inclusive, contiguous coverage
        takes: Take[]      // stack — LAST is the kept take
        accepted: boolean
    }
    class Take {
        file: string       // pNNN_tNN.wav
        durationSec: number
        recovered?: true
    }
    class AudioFormat {
        sampleRate: number
        channels: 1
        bits: 24
    }
    class ScriptUnit {
        text: string
        cue: string      // amber [VISUAL] footnote
        chapter: string
    }
    Session "1" --> "*" Passage
    Session "1" --> "*" ScriptUnit
    Session --> AudioFormat
    Passage "1" --> "*" Take
```

**Invariants:** passages tile `units` contiguously (no gaps/overlaps); merge/split are only legal
on take-less passages; `cursor` is always clamped to a valid index.

---

## GAP AUDIT (iteration 2 — found by drawing the diagrams above)

| # | Gap (how the diagram exposed it) | Severity | Fix |
|---|---|---|---|
| 0 | No back/ESC from Grouping or Booth (screen FSM had one-way edges) | UX-blocking | ESC paths added (prev. hotfix), now in diagram 1 |
| 1 | `revertArmed` survives passage navigation → second R discards the WRONG passage's take (state diagram had no disarm edges) | **data loss** | disarm on nav / record / accept / Esc |
| 2 | Undo window survives a new recording → undone take lands ON TOP of the newer take (UndoWindow had no cancel edge) | wrong kept take | cancel undo when a new take is recorded on that passage |
| 3 | Second export renames over `replaced/voice.mp3.bak` → destroys the original backup (export diagram, backup node) | **data loss** | timestamped backup names `<name>.<unix-ts>.bak` |
| 4 | Dead-mic recording writes silence with no warning (sequence diagram, frame loop) | silent failure | NO SIGNAL chip after 2 s of peak < 0.001 |
| 5 | Review ledger: no keyboard scroll, rows not actionable | UX | J/K selection + Enter/click jumps to passage in Booth |
| 6 | LOAD list scanned once per app launch | UX | rescan on entry + RESCAN button |
| 7 | Nothing tells you when every passage is recorded | UX | green completion chip → Review |
| 8 | Teleprompter "— end of script —" branch unreachable (cursor clamped) | dead code | removed |
| 9 | Keyboard-only controls — nothing visibly clickable | founder directive | Btn layer on every screen (this iteration) |
| 10 | Teleprompter long passages overflow — text slides under the top rail and clips the amber cue line (fixed 26px font + `overflow: hidden` centering assumed short passages) | cue unreadable | font auto-scales 26→18px with passage length; margin-auto overflow-safe centering + scroll; ghost lines hidden >420 chars; cue line `flexShrink: 0` |
| 11 | Only the TOP take is discardable (R-R) — a bad middle take is stuck in the stack | founder directive | ✕ per take card, single click (5 s undo is the net), `discard_at` moves any take to `discarded/` (diagram 2) |
| 12 | REC button's circle ring | founder directive (cosmetic) | ring removed — bare glyph + label, live pulse moved to the glyph glow |
| 13 | Conditional ↶ UNDO button widened the control bar past the window edge — REVIEW clipped while the undo window was open | UX | UNDO moved next to the amber TAKE DISCARDED status line; control bar width never changes |
| 14 | Sample-rate pin blocked recording when the input device changed (AirPods 24 kHz session vs built-in 48 kHz) — even on an empty session | founder directive | gate removed; export resamples minority-rate takes to the highest take rate via ffmpeg before concat (diagram 5 NORM/RS); uniform sessions keep the pure sample-exact path |
| 15 | Transcript screen selection didn't carry into the booth — select row 1, BEGIN, land on the OLD cursor's passage (sel was local state, never written to `session.cursor`) | wrong passage | `begin()` saves `cursor = sel` before switching screens |
| 16 | Script text was read-only in the booth — wording tweaks meant editing files by hand | founder directive | click the teleprompter text (idle only) → inline textarea, one paragraph per unit; SAVE button (greyed until the draft changes) + always-active CANCEL, or ⌘S; propagates each changed unit to session.json, `script-units.json`, and `completed-videos/<slug>/script.md` (exact-match replace; warnings on the amber chip if a target is missing); Esc/CANCEL discards immediately; click-outside exits silently when clean, but a dirty draft pops SAVE CHANGES? (Save ⏎ / Discard esc) — never a silent data loss |
| 17 | Control bar overflowed again when VIEW TRANSCRIPT widened it (fixed px button metrics assumed short labels + wide window) | REVIEW clipped | button metrics viewport-scaled via `clamp()` (`.btn`, `.control-bar`, REC); key hints hide below 1340 px (keys still work) |
| 18 | App was unusable off the author's machine — episodes root, asset scope, and the edit write-back sink were all hardcoded to one workspace | OSS-blocking | project model: recents + OPEN FOLDER (config.rs), runtime asset-scope grants, `session.sourceFile` write-back target (legacy completed-videos sink removed) |
| 19 | Only pre-built script-units.json / chunks.json could be opened — a stranger has a script in a document, not our JSON | OSS-blocking | IMPORT SCRIPT… (.md/.txt) on LOAD: `units_from_document` → persisted units file → normal session; cue convention preserved |
| 20 | Export hard-required ffmpeg (mp3 step) — strangers won't have it installed | OSS-blocking | ffmpeg is optional: WAV always exports (pure Rust); mp3 encodes when `ffmpeg_available()`; mixed-rate-without-ffmpeg fails with a clear error naming the rates; Review shows a NO FFMPEG chip before export |
| 21 | No in-app documentation — a stranger has to find the README to learn the keys | onboarding | `?` modal cheat-sheet on every screen (HelpOverlay; swallows all keys while open so reading help can't trigger a recording), faint `?` corner button |

Future gaps: add the transition to the diagram FIRST, then implement, then append a row here.
