#!/usr/bin/env bash
# Capture the booth window (any Space) to the given path. Dev verification helper.
set -euo pipefail
OUT="${1:?usage: shoot-booth.sh <out.png>}"
WID=$("$HOME/.venvs/qwen3-tts/bin/python" - <<'EOF'
import Quartz
wins = Quartz.CGWindowListCopyWindowInfo(Quartz.kCGWindowListExcludeDesktopElements, Quartz.kCGNullWindowID)
for w in wins:
    if (w.get('kCGWindowOwnerName','') or '').lower() == 'booth' and w.get('kCGWindowName'):
        print(w['kCGWindowNumber']); break
EOF
)
[ -n "$WID" ] || { echo "booth window not found" >&2; exit 1; }
screencapture -x -l "$WID" "$OUT"
echo "$OUT"
