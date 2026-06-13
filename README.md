# ATTESTRUM // BOOTH

A macOS recording booth for reading scripts aloud — teleprompter-style, one
passage at a time, with a take stack, crash-safe lossless recording, and a
clean WAV/MP3 export at the end. Built for narrating long-form video scripts;
useful for anyone who records voice-over against a written script.

Everything is local. No accounts, no cloud, no telemetry. Your script, your
takes, and your exports never leave your machine.

> Status: pre-release. v1 targets macOS 13+ on Apple Silicon.

## What it does

1. **Load a script** and Booth splits it into recordable passages (sentence
   detection, ~600-character groupings, chapter-aware). Review and adjust the
   grouping in the transcript screen — merge, split, pick where to start.
2. **Record passage by passage.** Big teleprompter text, `R` to record and
   stop, takes stack up newest-first. With more than one take, click a take to
   pick the kept one. Bad take? Re-record, revert (`D D`), or delete any take
   with its ✕ — files are moved, never deleted, and a 5-second undo backs every
   removal.
3. **Trim it.** Selecting a take shows its waveform — click to set the play
   cursor, drag to select a span, `Del` to cut leading/trailing/mid dead air.
   Cuts are non-destructive (the WAV is never touched) and applied on play and
   at export.
4. **Edit the script inline.** Click the teleprompter text, fix the wording,
   save — the edit writes back to your script file.
5. **Export** a sample-perfect concatenated `voice.wav` (and `voice.mp3` when
   ffmpeg is installed), with prior outputs backed up first.

Recording is crash-safe: takes are 24-bit WAVs flushed continuously; a crash
mid-take is auto-repaired on next launch. Sessions resume exactly where you
left off.

## Transcribe (URL or file)

The Load screen also transcribes — paste a video link (YouTube / TikTok /
Instagram / Facebook) or pick a local audio/video file (`⌁ FILE`). It's a
**local, personal** feature: everything runs on your machine, any downloaded
media is transient and deleted immediately, and the output is **transcript text
only**.

- **URLs prefer existing captions.** If the video already has a caption track,
  Booth imports it (no transcription compute) — instant and tagged `manual-subs`
  or `auto-subs`. Only when there are no captions does it download the audio and
  run Whisper locally (`large-v3-turbo`, Metal GPU; the ~1.6 GB model downloads
  once on first use). Caption-skip is reliable mainly on YouTube; TikTok/IG/FB
  usually take the Whisper path.
- **Local files** always transcribe with Whisper.
- **Export** the result as TXT, SRT, VTT, JSON, CSV, HTML, DOCX, or PDF.
- Saved transcripts appear in a **TRANSCRIPTS** group on the Load screen,
  re-openable and exportable any time.

> YouTube now gates requests behind a bot check, so Booth passes your **Chrome**
> browser cookies to yt-dlp (`--cookies-from-browser`). Keep Chrome installed
> and signed in; macOS may ask once for keychain access.

## Key vocabulary

| Key | Action |
|---|---|
| `SPACE` | play / pause the selected take |
| `R` | record / stop |
| `D D` | revert newest take (double-tap; moves to `discarded/`) |
| click waveform · drag · `Del` | set play cursor · select a span · cut it |
| `✕` (take card) | delete that take (single click; 5 s undo) |
| click a take card | select it as the kept take |
| `U` | undo a revert/delete |
| `ENTER` | accept take ▸ next passage |
| `J` / `K` | next / previous passage |
| `G` | view transcript (merge `M` / split `S`) |
| `TAB` | review screen |
| `⌘E` / `⇧⌘E` | export / export partial |
| `ESC` | back |

Every key is also a visible button — the **KEY BINDINGS** button (or `?`)
shows this table in-app. Click the **device name in the top rail** to switch
recording input.

## Install

Download the latest `.dmg` from
[Releases](https://github.com/Attestrum/booth/releases/latest), drag
`Booth.app` to Applications, and launch. macOS will ask for microphone access
on your first recording.

`voice.mp3` export and mixed-sample-rate sessions use
[ffmpeg](https://ffmpeg.org) if it's installed (`brew install ffmpeg`); WAV
export is built in and always works.

## Build from source

```bash
git clone https://github.com/Attestrum/booth
cd booth
npm install
bash src-tauri/scripts/fetch-yt-dlp.sh   # one-time: fetch the yt-dlp sidecar
npm run tauri dev      # live dev (Rust + Vite HMR)
npm run tauri build    # release .app → src-tauri/target/release/bundle/macos/
```

Requires Rust (stable, via `rust-toolchain.toml`), Node 22+, and **cmake**
(`brew install cmake`) — the transcription engine builds whisper.cpp from source
with the Metal backend. The yt-dlp sidecar is git-ignored; the fetch script
above pulls the latest build (re-run it periodically as sites change). The
Whisper model is **not** bundled — it downloads to app-data on first use. To produce a
signed local build, export `APPLE_SIGNING_IDENTITY` with a certificate from
your own keychain — unsigned builds work but macOS resets the microphone
permission on every rebuild.

- Audio is captured in Rust (`cpal` + `hound`, 24-bit), never in the webview.
- Sessions live in a `booth/` folder next to your script: `session.json`
  (atomic writes), `takes/`, `discarded/`, `replaced/` (export backups).
- Mixed input sample rates within a session are allowed; export resamples
  minority-rate takes to the highest take rate before concatenating.
- `DESIGN.md` is the spec — every screen, state, and transition is diagrammed,
  and changes go diagram-first.

## License

Apache-2.0 OR MIT, at your option. Copyright © Hyper Beam Media LLC.

Booth is a tool from the [Attestrum](https://attestrum.com) studio.
