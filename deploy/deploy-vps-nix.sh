#!/usr/bin/env bash
set -euo pipefail

VPS_HOST="vps-production"
REMOTE_DIR="/opt/antigravity"
SERVICE_NAME="antigravity"

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"; }
success() { echo -e "${GREEN}✓${NC} $1"; }
error() { echo -e "${RED}✗${NC} $1" >&2; exit 1; }

[[ -f "flake.nix" ]] || error "Run from project root"
[[ -d "src-leptos/dist" ]] || error "Frontend not built. Run: cd src-leptos && trunk build --release"

log "📦 Building with Nix locally..."
NIX_PATH=$(nix build .#antigravity-server --no-link --print-out-paths 2>&1 | tail -1)
[[ -f "${NIX_PATH}/bin/antigravity-server" ]] || error "Build failed"
success "Built: ${NIX_PATH}"

log "🚀 Copying Nix closure to VPS (includes all dependencies)..."
nix copy --to "ssh://${VPS_HOST}" "${NIX_PATH}" || error "nix copy failed"
success "Closure copied"

log "📁 Syncing frontend assets..."
rsync -az --delete src-leptos/dist/ "${VPS_HOST}:${REMOTE_DIR}/dist/" || error "rsync failed"
success "Frontend synced"

log "🔄 Installing service on VPS..."
ssh "${VPS_HOST}" "
  set -e
  systemctl stop antigravity.service 2>/dev/null || true
  
  # Create symlink from nix store to /opt/antigravity
  ln -sf ${NIX_PATH}/bin/antigravity-server /opt/antigravity/antigravity-server
  
  cat > /etc/systemd/system/antigravity.service << EOF
[Unit]
Description=Antigravity AI Gateway
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/antigravity
ExecStart=${NIX_PATH}/bin/antigravity-server
Restart=always
RestartSec=5
TimeoutStopSec=30

Environment=RUST_LOG=info
Environment=ANTIGRAVITY_PORT=8045
Environment=ANTIGRAVITY_STATIC_DIR=/opt/antigravity/dist
Environment=ANTIGRAVITY_DATA_DIR=/root/.antigravity_tools

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload
  systemctl enable antigravity.service
  systemctl start antigravity.service
" || error "Remote install failed"
success "Service started"

log "🔍 Verifying..."
sleep 3
HEALTH=$(ssh "${VPS_HOST}" "curl -sf http://localhost:8045/api/health 2>/dev/null" || echo "FAILED")
if [[ "$HEALTH" != "FAILED" ]]; then
    success "Deployment successful!"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "  🌐 https://antigravity.quantumind.ru"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
else
    error "Health check failed! Check: ssh ${VPS_HOST} journalctl -u ${SERVICE_NAME} -n 50"
fi
