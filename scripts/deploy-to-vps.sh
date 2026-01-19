#!/usr/bin/env bash
set -euo pipefail

VPS_HOST="${VPS_HOST:-vps-production}"
BUILD_DIR="$(dirname "$(dirname "$(realpath "$0")")")"
BINARY_NAME="antigravity-server"
VPS_BINARY_PATH="/usr/local/bin/${BINARY_NAME}"
SERVICE_NAME="antigravity-server"

log() { echo "[$(date '+%H:%M:%S')] $*"; }
error() { echo "[$(date '+%H:%M:%S')] ❌ $*" >&2; exit 1; }

cd "$BUILD_DIR"

log "🔨 Building release binary..."
cargo build --release -p antigravity-server

LOCAL_BINARY="target/release/${BINARY_NAME}"
[[ -f "$LOCAL_BINARY" ]] || error "Binary not found at $LOCAL_BINARY"

BINARY_SIZE=$(du -h "$LOCAL_BINARY" | cut -f1)
log "📦 Binary size: $BINARY_SIZE"

log "📤 Uploading binary to VPS..."
scp "$LOCAL_BINARY" "${VPS_HOST}:/tmp/${BINARY_NAME}.new"

log "🚀 Deploying on VPS (zero-downtime)..."
ssh "$VPS_HOST" bash -s <<'REMOTE_SCRIPT'
set -euo pipefail

BINARY_NAME="antigravity-server"
BINARY_PATH="/usr/local/bin/${BINARY_NAME}"
SERVICE_NAME="antigravity-server"
NEW_BINARY="/tmp/${BINARY_NAME}.new"

log() { echo "[$(date '+%H:%M:%S')] $*"; }

chmod +x "$NEW_BINARY"
VERSION=$("$NEW_BINARY" --version 2>/dev/null | head -1 || echo "unknown")
log "   New version: $VERSION"

log "🔄 Stopping service..."
systemctl stop "$SERVICE_NAME" || true
sleep 1

log "📝 Replacing binary..."
mv "$NEW_BINARY" "$BINARY_PATH"

log "🚀 Starting service..."
systemctl start "$SERVICE_NAME"
sleep 2

if systemctl is-active --quiet "$SERVICE_NAME"; then
    HEALTH=$(curl -sf http://127.0.0.1:8045/healthz 2>/dev/null || echo '{"status":"unknown"}')
    log "✅ Service healthy: $HEALTH"
else
    log "❌ Service failed to start!"
    journalctl -u "$SERVICE_NAME" --no-pager -n 20
    exit 1
fi
REMOTE_SCRIPT

log "✅ VPS deployment complete!"

log "📊 Verification:"
ssh "$VPS_HOST" "curl -s http://127.0.0.1:8045/healthz"
echo ""
