#!/usr/bin/env bash
# Antigravity Manager - Unified Deployment Script
# Usage: ./deploy.sh <command> [options]
#
# Commands:
#   container   Build and deploy container to VPS
#   binary      Build and deploy binary to VPS  
#   local       Zero-downtime deploy on local machine
#   rollback    Rollback to previous version on VPS
#   status      Show deployment status on VPS
#
# Options:
#   --remote-build    Build on VPS instead of locally (container only)
#   --dry-run         Show what would be done without executing
#   --rootless        Use rootless Podman (user systemd)
#   -h, --help        Show this help

set -euo pipefail

# =============================================================================
# Configuration
# =============================================================================
VPS_HOST="${VPS_HOST:-vps-production}"
IMAGE_NAME="antigravity-manager"
BINARY_NAME="antigravity-server"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Paths
QUADLET_FILE="deploy/antigravity.container"
REMOTE_QUADLET_DIR="/etc/containers/systemd"
REMOTE_QUADLET_DIR_ROOTLESS="~/.config/containers/systemd"
REMOTE_BUILD_DIR="~/antigravity-build"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# State
DRY_RUN=false
REMOTE_BUILD=false
ROOTLESS=false
COMMAND=""

# =============================================================================
# Helpers
# =============================================================================
log()     { echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $1"; }
success() { echo -e "${GREEN}✓${NC} $1"; }
warn()    { echo -e "${YELLOW}⚠${NC} $1"; }
error()   { echo -e "${RED}✗${NC} $1" >&2; exit 1; }
info()    { echo -e "${CYAN}ℹ${NC} $1"; }

dry_run() {
    if [[ "$DRY_RUN" == "true" ]]; then
        info "[DRY-RUN] $*"
        return 0
    fi
    "$@"
}

get_version() {
    git describe --tags --always --dirty 2>/dev/null || echo "dev"
}

# =============================================================================
# Command implementations (stubs - will be filled)
# =============================================================================
cmd_container() { error "Not implemented yet"; }
cmd_binary()    { error "Not implemented yet"; }
cmd_local()     { error "Not implemented yet"; }
cmd_rollback()  { error "Not implemented yet"; }
cmd_status()    { error "Not implemented yet"; }

show_help() {
    sed -n '2,16p' "$0" | sed 's/^# \?//'
    echo ""
    echo "Examples:"
    echo "  ./deploy.sh container              # Build locally, ship to VPS"
    echo "  ./deploy.sh container --remote-build  # Build on VPS (faster)"
    echo "  ./deploy.sh binary                 # Deploy binary only"
    echo "  ./deploy.sh rollback               # Rollback to previous version"
    echo "  ./deploy.sh status                 # Check VPS deployment status"
}

# =============================================================================
# Argument parsing
# =============================================================================
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            container|binary|local|rollback|status)
                COMMAND="$1"
                ;;
            --remote-build)
                REMOTE_BUILD=true
                ;;
            --dry-run)
                DRY_RUN=true
                ;;
            --rootless)
                ROOTLESS=true
                ;;
            -h|--help)
                show_help
                exit 0
                ;;
            *)
                error "Unknown option: $1. Use --help for usage."
                ;;
        esac
        shift
    done

    [[ -n "$COMMAND" ]] || { show_help; exit 1; }
}

# =============================================================================
# Main
# =============================================================================
main() {
    cd "$SCRIPT_DIR"
    [[ -f "Containerfile" ]] || error "Run from project root"

    parse_args "$@"

    VERSION=$(get_version)
    log "Antigravity Deploy v${VERSION}"
    
    if [[ "$DRY_RUN" == "true" ]]; then
        warn "DRY-RUN mode enabled"
    fi

    case "$COMMAND" in
        container) cmd_container ;;
        binary)    cmd_binary ;;
        local)     cmd_local ;;
        rollback)  cmd_rollback ;;
        status)    cmd_status ;;
    esac
}

main "$@"
