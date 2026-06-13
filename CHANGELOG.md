# Changelog

## v0.1.2 — 2026-06-13

- **Import several scripts from one folder:** importing a `.md`/`.txt` into a
  folder that already holds a booth session no longer fails with "this folder
  already has a booth session." The new script gets its own subfolder (named
  after the document) and becomes its own episode — so you can keep multiple
  scripts in `~/Downloads` (or anywhere) and import each. Existing sessions are
  untouched and still open from the list.
- **Transcription — paste a link or pick a file:** the Load screen now
  transcribes. Paste a YouTube / TikTok / Instagram / Facebook link or choose a
  local audio/video file. URLs import an existing caption track when there is one
  (instant, no compute, tagged `manual-subs` / `auto-subs`) and otherwise
  download the audio and run Whisper locally (`large-v3-turbo` on the Metal GPU;
  the model downloads once on first use). Results are timestamped, saved to a
  re-openable **TRANSCRIPTS** library, and exportable as TXT / SRT / VTT / JSON /
  CSV / HTML / DOCX / PDF. Local, personal, transcript-only: downloaded media is
  transient and deleted immediately. (YouTube uses your Chrome cookies to clear
  its bot check.)
- **Legibility pass — no more faded text:** the secondary text floor was raised
  (`--dim-cyan` 0.45→0.72) and a dedicated soft tier (`--dim-cyan-soft`) replaces
  the old habit of stacking `opacity` on top of dim (which dragged some rows as
  low as ~0.18). Group headers, inactive session/passage rows, take-stack cards,
  stats, and key hints now all read clearly. `--faint-cyan` stays borders/fills
  only; disabled controls are the only dimmed exemption (gap #28).
- **No silence between beats on export:** passages now abut directly in the
  concatenated `voice.wav` (the 350 ms inter-passage "breath room" is gone) —
  trim any per-take dead air with the waveform editor.
- **Exports are named after the document, next to it:** Export now writes
  `<document>.wav` / `<document>.mp3` directly in the source folder (not a generic
  `voice.*` buried in `narration/`). A re-export never overwrites — it adds
  ` (1)`, ` (2)`, … so every render is kept as its own file.
- **Gapless, click-free cut playback:** playing a take with cuts no longer
  clicks or stutters at the splices. Playback now decodes the take once and
  schedules the kept spans back-to-back on the Web Audio clock (sample-accurate,
  short anti-click edge fades) instead of seeking an `<audio>` element — and the
  playhead is driven off the audio clock, so it sweeps smoothly across cuts.
- **Audacity-style transport keys:** `SPACE` is now **Play / Pause** (resumes
  in place), `R` is **Record / Stop**, and Revert is **`D` `D`** (double-tap).
  This matches every audio editor — reach for Space to audition a take, not to
  arm one. Buttons and the `?` cheat-sheet updated to match.
- **Record cue no longer bleeds into the take:** the "going hot" sound now
  finishes before the mic opens, so it can't appear at the head of a recording.
- **Selectable takes:** with more than one take on a passage, click any take card
  to make it the kept one — it becomes what plays, what Accept confirms, and what
  Export ships. New recordings still auto-select the newest; R·R still discards
  the newest take.
- **Inline cuts (remove dead air & flubs), Audacity-style:** when a take is
  selected, its waveform shows in the strip. Click to set the play cursor, drag
  to select, `Del` to cut — leading/trailing silence OR a flub in the middle.
  The cut audio leaves the timeline (the waveform closes the gap, ripple-delete)
  and a thin **break stub** marks the splice; click a stub to restore that cut,
  or `↩ RESTORE` to revert the whole take. Multiple cuts per take. Non-destructive
  — the original WAV is untouched; cuts apply on Play and sample-exactly at Export.
- Input-device picker: click the device name in the booth's top rail to choose
  any input (or System default). The choice persists and falls back to the OS
  default when the device disappears.
- The `?` corner dot is now a proper **KEY BINDINGS** button; the `?` key still
  toggles the cheat-sheet.
- Readability rule: the faint color tier is reserved for borders/fills — all
  text now sits at the dim tier or brighter (take labels, recorded counter,
  list hints, help footer, teleprompter ghosts).

## v0.1.1 — 2026-06-12

- Imported sessions are named after the script file, not its folder (importing
  from `~/Downloads` no longer produces a session called "Downloads").
- Brightened the TAKES / RECORDED labels.

## v0.1.0 — 2026-06-12

First public release.

- Teleprompter recording booth: passage-by-passage takes with stack, revert,
  per-take delete (5 s undo), crash-safe 24-bit WAV capture in Rust.
- Import a `.md`/`.txt` script — headings become chapters, paragraphs become
  recordable passages; merge/split in the transcript screen.
- Inline script editing with write-back to the source document.
- Export: sample-perfect `voice.wav` always; `voice.mp3` and mixed-rate
  resampling when ffmpeg is installed.
- Project folders with recents; everything stays local — no accounts, no
  telemetry.
- Signed + notarized `.dmg` for macOS 13+ (Apple Silicon).
