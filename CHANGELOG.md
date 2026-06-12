# Changelog

## Unreleased

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
