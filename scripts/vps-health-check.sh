#!/usr/bin/env bash
set -euo pipefail

TELEGRAM_BOT_TOKEN="${TELEGRAM_BOT_TOKEN:-}"
TELEGRAM_CHAT_ID="${TELEGRAM_CHAT_ID:-432567587}"
SERVICE_NAME="antigravity-server"
HEALTH_URL="http://127.0.0.1:8045/healthz"
STATE_FILE="/tmp/antigravity-health-state"

send_telegram() {
    local message="$1"
    [[ -z "$TELEGRAM_BOT_TOKEN" ]] && return 0
    curl -s -X POST "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/sendMessage" \
        -d "chat_id=${TELEGRAM_CHAT_ID}" \
        -d "text=${message}" \
        -d "parse_mode=HTML" > /dev/null 2>&1 || true
}

get_previous_state() {
    cat "$STATE_FILE" 2>/dev/null || echo "unknown"
}

save_state() {
    echo "$1" > "$STATE_FILE"
}

PREV_STATE=$(get_previous_state)

if ! systemctl is-active --quiet "$SERVICE_NAME"; then
    if [[ "$PREV_STATE" != "down" ]]; then
        send_telegram "🔴 <b>Antigravity Server DOWN</b>
Service $SERVICE_NAME is not running
Host: $(hostname)"
        save_state "down"
    fi
    exit 1
fi

HEALTH=$(curl -sf "$HEALTH_URL" --max-time 5 2>/dev/null || echo '{"status":"error"}')

if echo "$HEALTH" | grep -q '"ok"'; then
    if [[ "$PREV_STATE" == "down" ]] || [[ "$PREV_STATE" == "error" ]]; then
        send_telegram "🟢 <b>Antigravity Server RECOVERED</b>
Service is healthy again
Host: $(hostname)"
    fi
    save_state "ok"
    exit 0
else
    if [[ "$PREV_STATE" != "error" ]]; then
        send_telegram "🟡 <b>Antigravity Server DEGRADED</b>
Health check failed: $HEALTH
Host: $(hostname)"
        save_state "error"
    fi
    exit 1
fi
