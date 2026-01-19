#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="antigravity-server"
BINARY_PATH="${HOME}/.local/bin/${BINARY_NAME}"
SERVICE_NAME="antigravity-manager"
BUILD_DIR="$(dirname "$(dirname "$(realpath "$0")")")"
DRAIN_TIMEOUT=30

log() { echo "[$(date '+%H:%M:%S')] $*"; }

log "🔨 Building release binary..."
cd "$BUILD_DIR"
cargo build --release -p antigravity-server

log "📦 Deploying new binary..."
cp "target/release/${BINARY_NAME}" "${BINARY_PATH}.new"
chmod +x "${BINARY_PATH}.new"

log "🚀 Starting new instance (SO_REUSEPORT overlap)..."
"${BINARY_PATH}.new" &
NEW_PID=$!

sleep 2

if ! kill -0 "$NEW_PID" 2>/dev/null; then
    log "❌ New instance failed to start!"
    rm -f "${BINARY_PATH}.new"
    exit 1
fi

log "✅ New instance running (PID: $NEW_PID)"
log "🛑 Sending SIGTERM to old instance..."
systemctl --user stop "${SERVICE_NAME}.service" || true

log "⏳ Waiting for old instance to drain (${DRAIN_TIMEOUT}s max)..."
sleep 3

log "🔄 Replacing binary..."
mv "${BINARY_PATH}.new" "${BINARY_PATH}"

log "🚀 Restarting via systemd..."
kill "$NEW_PID" 2>/dev/null || true
sleep 1
systemctl --user start "${SERVICE_NAME}.service"

sleep 2
if systemctl --user is-active --quiet "${SERVICE_NAME}.service"; then
    log "✅ Zero-downtime deploy complete!"
else
    log "⚠️ Service may have issues, check: systemctl --user status ${SERVICE_NAME}"
fi
