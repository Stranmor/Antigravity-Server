#!/usr/bin/env bash
# Antigravity Manager - Fast VPS Deployment
# Usage: ./deploy/deploy-vps-fast.sh
#
# This script uses BUILD LOCALLY, SHIP BINARY approach:
# 1. Uses pre-built binary from target/release/ (must run cargo build first!)
# 2. Builds minimal runtime container (~50MB vs ~2GB)
# 3. Ships to VPS in ~30 seconds

set -euo pipefail

# Configuration
VPS_HOST="vps-production"
IMAGE_NAME="antigravity-manager"
IMAGE_TAG="latest"
CONTAINERFILE="Containerfile.runtime"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"; }
success() { echo -e "${GREEN}✓${NC} $1"; }
error() { echo -e "${RED}✗${NC} $1" >&2; exit 1; }

# Verify prerequisites
[[ -f "$CONTAINERFILE" ]] || error "Run from project root ($CONTAINERFILE not found)"
[[ -f "target/x86_64-unknown-linux-musl/release/antigravity-server" ]] || error "MUSL binary not found! Run: cargo build --release -p antigravity-server --target x86_64-unknown-linux-musl"
[[ -d "src-leptos/dist" ]] || error "Frontend not built! Run: cd src-leptos && trunk build --release"

# Step 1: Build runtime container (uses local binary, no Rust inside)
log "📦 Building runtime container..."
podman build -f "$CONTAINERFILE" -t "${IMAGE_NAME}:${IMAGE_TAG}" . || error "Build failed"
success "Container built: ${IMAGE_NAME}:${IMAGE_TAG}"

# Step 2: Save and ship
log "🚀 Shipping to VPS..."
podman save --format docker-archive "${IMAGE_NAME}:${IMAGE_TAG}" | \
    ssh "${VPS_HOST}" "podman load" || error "Failed to ship image"
success "Image shipped"

# Step 3: Restart service
log "🔄 Restarting service..."
ssh "${VPS_HOST}" "systemctl restart antigravity.service" || error "Failed to restart"
success "Service restarted"

# Step 4: Verify
log "🔍 Verifying..."
sleep 3
HEALTH=$(ssh "${VPS_HOST}" "curl -sf http://localhost:8045/api/health 2>/dev/null" || echo "FAILED")
if [[ "$HEALTH" != "FAILED" ]]; then
    success "Deployment successful!"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "  🌐 https://antigravity.quantumind.ru"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
else
    error "Health check failed! Check: ssh ${VPS_HOST} journalctl -u antigravity -n 50"
fi
