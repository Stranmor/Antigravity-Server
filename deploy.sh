#!/usr/bin/env bash
set -euo pipefail

VPS_HOST="vps-production"
SERVICE_NAME="antigravity"
REMOTE_DIR="/opt/antigravity"
PORT=8045
URL="https://antigravity.quantumind.ru"

RED='\033[0;31m'; GREEN='\033[0;32m'; BLUE='\033[0;34m'; NC='\033[0m'
log()     { echo -e "${BLUE}[deploy]${NC} $*"; }
success() { echo -e "${GREEN}[ok]${NC} $*"; }
error()   { echo -e "${RED}[error]${NC} $*" >&2; exit 1; }

get_version() { git describe --tags --always --dirty 2>/dev/null || echo "unknown"; }

cmd_deploy_local() {
    local SERVICE_LOCAL="antigravity-manager"
    local BINARY_PATH="${HOME}/.local/bin/antigravity-server"
    local PORT_LOCAL=8045

    log "Building release binary..."
    cargo build --release -p antigravity-server
    success "Built: target/release/antigravity-server"

    # Replace binary (rm first — running process holds old inode)
    rm -f "${BINARY_PATH}"
    cp "target/release/antigravity-server" "${BINARY_PATH}"
    chmod +x "${BINARY_PATH}"
    success "Binary replaced"

    # Socket activation = zero-downtime restart
    # Socket stays open, kernel buffers connections during restart
    log "Restarting service (socket-activated, zero-downtime)..."
    systemctl --user restart "${SERVICE_LOCAL}"

    local ready=false
    for _ in $(seq 1 15); do
        sleep 1
        if curl -sf "http://localhost:${PORT_LOCAL}/v1/models" > /dev/null 2>&1; then
            ready=true
            break
        fi
    done

    if ! $ready; then
        error "Service failed health check after 15s — check: journalctl --user -u ${SERVICE_LOCAL}"
    fi

    success "Local deploy complete (v$(get_version))"
}

cmd_deploy() {
    log "Building Nix closure..."
    [[ -f flake.nix ]] || error "flake.nix not found"
    [[ -d src-leptos/dist ]] || error "src-leptos/dist/ not found — build frontend first"

    NIX_PATH=$(nix build .#antigravity-server --no-link --print-out-paths)
    [[ -x "${NIX_PATH}/bin/antigravity-server" ]] || error "Binary not found at ${NIX_PATH}/bin/antigravity-server"
    success "Built: ${NIX_PATH}"

    log "Copying Nix closure to ${VPS_HOST}..."
    nix copy --to "ssh://${VPS_HOST}" "${NIX_PATH}"
    success "Closure copied"

    log "Syncing frontend assets..."
    rsync -az --delete src-leptos/dist/ "${VPS_HOST}:${REMOTE_DIR}/dist/"
    success "Frontend synced"

    log "Deploying on VPS..."
    ssh "${VPS_HOST}" bash -s -- "${NIX_PATH}" "${REMOTE_DIR}" "${PORT}" <<'REMOTE_SCRIPT'
        set -euo pipefail
        NIX_PATH="$1"; REMOTE_DIR="$2"; PORT="$3"
        mkdir -p "${REMOTE_DIR}"

        # Backup current version
        CURRENT=$(readlink -f "${REMOTE_DIR}/antigravity-server" 2>/dev/null || true)
        [[ -n "${CURRENT}" ]] && echo "${CURRENT}" > "${REMOTE_DIR}/.previous"

        ln -sf "${NIX_PATH}/bin/antigravity-server" "${REMOTE_DIR}/antigravity-server"

        # Create default .env if missing (DATABASE_URL etc. live here, not in unit)
        if [[ ! -f "${REMOTE_DIR}/.env" ]]; then
            cat > "${REMOTE_DIR}/.env" <<ENVEOF
RUST_LOG=info
ANTIGRAVITY_PORT=${PORT}
ANTIGRAVITY_STATIC_DIR=${REMOTE_DIR}/dist
ANTIGRAVITY_DATA_DIR=/root/.antigravity_tools
# DATABASE_URL=postgres://antigravity:password@127.0.0.1/antigravity
ENVEOF
        fi

        if [[ -f /etc/NIXOS ]]; then
            # NixOS: systemd units are declarative (read-only /etc/systemd/system).
            # Unit is managed via configuration.nix — just restart the service.
            systemctl restart antigravity.service
        else
            cat > /etc/systemd/system/antigravity.service <<EOF
[Unit]
Description=Antigravity AI Gateway
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=${REMOTE_DIR}
ExecStart=${NIX_PATH}/bin/antigravity-server
Restart=always
RestartSec=5
TimeoutStopSec=30
EnvironmentFile=${REMOTE_DIR}/.env

[Install]
WantedBy=multi-user.target
EOF
            systemctl daemon-reload
            systemctl enable antigravity.service
            systemctl restart antigravity.service
        fi
REMOTE_SCRIPT
    success "Service restarted"

    log "Waiting for health check (up to 60s — initial quota refresh may take time)..."
    for i in $(seq 1 15); do
        if ssh "${VPS_HOST}" "curl -sf http://localhost:${PORT}/api/health" >/dev/null 2>&1; then
            success "Deploy complete: ${URL}"
            return 0
        fi
        sleep 4
    done
    error "Health check failed after 60s — check logs: ./deploy.sh logs"
}

cmd_rollback() {
    log "Rolling back on ${VPS_HOST}..."
    ssh "${VPS_HOST}" bash -s -- "${REMOTE_DIR}" "${PORT}" <<'REMOTE_SCRIPT'
        set -euo pipefail
        REMOTE_DIR="$1"; PORT="$2"
        PREV_FILE="${REMOTE_DIR}/.previous"
        [[ -f "${PREV_FILE}" ]] || { echo "No previous version found" >&2; exit 1; }

        PREV_BIN=$(cat "${PREV_FILE}")
        [[ -x "${PREV_BIN}" ]] || { echo "Previous binary missing: ${PREV_BIN}" >&2; exit 1; }

        PREV_NIX=$(dirname "$(dirname "${PREV_BIN}")")
        ln -sf "${PREV_BIN}" "${REMOTE_DIR}/antigravity-server"

        if [[ ! -f /etc/NIXOS ]]; then
            sed -i "s|^ExecStart=.*|ExecStart=${PREV_BIN}|" /etc/systemd/system/antigravity.service
            systemctl daemon-reload
        fi
        systemctl restart antigravity.service
REMOTE_SCRIPT

    for i in $(seq 1 15); do
        if ssh "${VPS_HOST}" "curl -sf http://localhost:${PORT}/api/health" >/dev/null 2>&1; then
            success "Rollback complete: ${URL}"
            return 0
        fi
        sleep 4
    done
    error "Rollback health check failed after 60s"
}

cmd_status() {
    log "Service status:"
    ssh "${VPS_HOST}" "systemctl status ${SERVICE_NAME} --no-pager" || true
    echo ""
    log "Health check:"
    ssh "${VPS_HOST}" "curl -sf http://localhost:${PORT}/api/health" && echo "" || echo "UNHEALTHY"
    echo ""
    log "Current binary:"
    ssh "${VPS_HOST}" "readlink ${REMOTE_DIR}/antigravity-server 2>/dev/null || echo 'not found'"
    PREV=$(ssh "${VPS_HOST}" "cat ${REMOTE_DIR}/.previous 2>/dev/null || true")
    [[ -n "${PREV}" ]] && log "Previous: ${PREV}"
}

cmd_logs() {
    local lines=""
    shift || true
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -n) lines="$2"; shift 2 ;;
            *)  shift ;;
        esac
    done
    if [[ -n "${lines}" ]]; then
        ssh "${VPS_HOST}" "journalctl -u ${SERVICE_NAME} --no-pager -n ${lines} -f"
    else
        ssh "${VPS_HOST}" "journalctl -u ${SERVICE_NAME} --no-pager -f"
    fi
}

usage() {
    cat <<EOF
Antigravity Deploy v$(get_version)

Usage: ./deploy.sh <command> [options]

Commands:
  deploy       Build and deploy to VPS via Nix closure
  deploy-local Zero-downtime local deploy (socket activation)
  rollback     Rollback to previous version on VPS
  status       Show VPS service status and health
  logs         Stream VPS service logs (use -n N for line count)
  help         Show this help

Target: ${URL}
EOF
}

echo -e "${BLUE}Antigravity Deploy v$(get_version)${NC}"

case "${1:-help}" in
    deploy)       cmd_deploy ;;
    deploy-local) cmd_deploy_local ;;
    rollback)     cmd_rollback ;;
    status)       cmd_status ;;
    logs)         cmd_logs "$@" ;;
    help|*)       usage ;;
esac
