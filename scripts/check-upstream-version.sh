#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
UPSTREAM_DIR="$PROJECT_DIR/vendor/antigravity-upstream"
STATE_FILE="/tmp/antigravity-upstream-version"

source ~/.config/system-notify/config 2>/dev/null || true
TELEGRAM_CHAT_ID="${TELEGRAM_CHAT_ID:-432567587}"

send_telegram() {
    local message="$1"
    [[ -z "${TELEGRAM_BOT_TOKEN:-}" ]] && return 0
    curl -s -X POST "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage" \
        -d "chat_id=${TELEGRAM_CHAT_ID}" \
        -d "text=${message}" \
        -d "parse_mode=HTML" > /dev/null 2>&1 || true
}

cd "$UPSTREAM_DIR"

git fetch origin --quiet 2>/dev/null

CURRENT=$(git describe --tags --always HEAD 2>/dev/null || git rev-parse --short HEAD)
LATEST=$(git describe --tags --always origin/main 2>/dev/null || git rev-parse --short origin/main)
PREVIOUS=$(cat "$STATE_FILE" 2>/dev/null || echo "unknown")

if [[ "$LATEST" != "$PREVIOUS" ]] && [[ "$LATEST" != "$CURRENT" ]]; then
    COMMITS_BEHIND=$(git rev-list --count HEAD..origin/main 2>/dev/null || echo "?")
    
    send_telegram "🔄 <b>Antigravity Upstream Update</b>

New version available: <code>$LATEST</code>
Current: <code>$CURRENT</code>
Commits behind: $COMMITS_BEHIND

To update:
<code>cd $UPSTREAM_DIR && git pull</code>"

    echo "$LATEST" > "$STATE_FILE"
    echo "New upstream version: $LATEST (was: $CURRENT)"
else
    echo "Up to date: $CURRENT"
fi
