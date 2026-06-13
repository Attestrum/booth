#!/usr/bin/env bash
# Fetch the yt-dlp sidecar into src-tauri/binaries/ for the transcription
# feature. The binary is git-ignored (~30 MB, and it goes stale) — run this once
# after cloning, and re-run periodically to keep yt-dlp current as sites change.
#
# Platform is auto-detected from `uname`, so this works both on a dev Mac and on
# the Windows CI runner (Git Bash). Each platform's sidecar is named by its full
# Rust target triple, matching tauri's externalBin convention and resolve_bin().
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$HERE/binaries"

case "$(uname -s)" in
  Darwin)
    NAME="yt-dlp-aarch64-apple-darwin"
    ASSET="yt-dlp_macos"
    ;;
  MINGW* | MSYS* | CYGWIN* | Windows_NT)
    NAME="yt-dlp-x86_64-pc-windows-msvc.exe"
    ASSET="yt-dlp.exe"
    ;;
  Linux)
    NAME="yt-dlp-x86_64-unknown-linux-gnu"
    ASSET="yt-dlp_linux"
    ;;
  *)
    echo "Unsupported platform: $(uname -s)" >&2
    exit 1
    ;;
esac

OUT="$HERE/binaries/$NAME"
URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/$ASSET"

echo "Downloading yt-dlp (latest) → $OUT"
curl -fSL "$URL" -o "$OUT"
chmod +x "$OUT" || true
echo "yt-dlp $("$OUT" --version)"
