#!/usr/bin/env bash
# Antigravity Manager - VPS Deployment Script
# Usage: ./deploy/deploy-vps.sh [--skip-build]
#
# This script:
# 1. Builds container image locally via podman
# 2. Saves and ships image to VPS via SSH
# 3. Loads image on VPS
# 4. Installs/updates Quadlet systemd unit
# 5. Restarts service with zero-downtime (if possible)

set -euo pipefail

# Configuration
VPS_HOST="vps-production"
IMAGE_NAME="antigravity-manager"
IMAGE_TAG="latest"
QUADLET_FILE="deploy/antigravity.container"
REMOTE_QUADLET_DIR="/etc/containers/systemd"
REMOTE_IMAGE_PATH="/tmp/${IMAGE_NAME}.tar"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() { echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"; }
success() { echo -e "${GREEN}✓${NC} $1"; }
warn() { echo -e "${YELLOW}⚠${NC} $1"; }
error() { echo -e "${RED}✗${NC} $1" >&2; exit 1; }

# Parse arguments
SKIP_BUILD=false
for arg in "$@"; do
    case $arg in
        --skip-build) SKIP_BUILD=true ;;
        --help|-h)
            echo "Usage: $0 [--skip-build]"
            echo "  --skip-build  Skip local build, use existing image"
            exit 0
            ;;
    esac
done

# Verify we're in project root
[[ -f "Containerfile" ]] || error "Run from project root (Containerfile not found)"

# Step 1: Build image locally
if [[ "$SKIP_BUILD" == "false" ]]; then
    log "📦 Building container image..."
    podman build -t "${IMAGE_NAME}:${IMAGE_TAG}" . || error "Build failed"
    success "Image built: ${IMAGE_NAME}:${IMAGE_TAG}"
else
    log "⏭️  Skipping build (--skip-build)"
    podman image exists "${IMAGE_NAME}:${IMAGE_TAG}" || error "Image not found locally"
fi

# Step 2: Save image to tarball
log "💾 Saving image to tarball..."
podman save -o "/tmp/${IMAGE_NAME}.tar" "${IMAGE_NAME}:${IMAGE_TAG}" || error "Failed to save image"
success "Image saved: /tmp/${IMAGE_NAME}.tar ($(du -h /tmp/${IMAGE_NAME}.tar | cut -f1))"

# Step 3: Ship to VPS
log "🚀 Shipping image to VPS (${VPS_HOST})..."
scp "/tmp/${IMAGE_NAME}.tar" "${VPS_HOST}:${REMOTE_IMAGE_PATH}" || error "Failed to copy image"
success "Image shipped to ${VPS_HOST}:${REMOTE_IMAGE_PATH}"

# Step 4: Load image on VPS
log "📥 Loading image on VPS..."
ssh "${VPS_HOST}" "podman load -i ${REMOTE_IMAGE_PATH} && rm -f ${REMOTE_IMAGE_PATH}" || error "Failed to load image"
success "Image loaded on VPS"

# Step 5: Install Quadlet unit
log "📄 Installing Quadlet unit..."
scp "${QUADLET_FILE}" "${VPS_HOST}:${REMOTE_QUADLET_DIR}/antigravity.container" || error "Failed to copy Quadlet"
success "Quadlet installed"

# Step 6: Reload systemd and restart service
log "🔄 Restarting service..."
ssh "${VPS_HOST}" "systemctl daemon-reload && systemctl restart antigravity.service" || error "Failed to restart"
success "Service restarted"

# Step 7: Verify deployment
log "🔍 Verifying deployment..."
sleep 5

HEALTH=$(ssh "${VPS_HOST}" "curl -sf http://localhost:8045/api/health 2>/dev/null" || echo "FAILED")
if [[ "$HEALTH" != "FAILED" ]]; then
    success "Deployment successful!"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "  🌐 WebUI:  https://antigravity.quantumind.ru"
    echo -e "  📊 Health: ${HEALTH}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
else
    error "Health check failed! Check logs: ssh ${VPS_HOST} journalctl -u antigravity -n 50"
fi

# Cleanup local tarball
rm -f "/tmp/${IMAGE_NAME}.tar"
