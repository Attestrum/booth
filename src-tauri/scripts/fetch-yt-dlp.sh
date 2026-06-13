#!/usr/bin/env bash
# Fetch the yt-dlp macOS sidecar into src-tauri/binaries/ for the transcription
# feature. The binary is git-ignored (36 MB, and it goes stale) — run this once
# after cloning, and re-run periodically to keep yt-dlp current as sites change.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$HERE/binaries/yt-dlp-aarch64-apple-darwin"
URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"

mkdir -p "$HERE/binaries"
echo "Downloading yt-dlp (latest) → $OUT"
curl -fSL "$URL" -o "$OUT"
chmod +x "$OUT"
echo "yt-dlp $("$OUT" --version)"
