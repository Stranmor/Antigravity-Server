#!/usr/bin/env bash
# ============================================================================
# Antigravity WARP Isolation - Deployment Script
# ============================================================================
# Deploys WARP isolation infrastructure to production VPS.
# Handles:
#   - Script deployment
#   - WARP key generation
#   - Container generation
#   - Systemd reload
#   - Verification
#
# Usage:
#   ./deploy-warp-isolation.sh [VPS_HOST]
#
# Default VPS: vps-production (from SSH config)
# ============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VPS_HOST="${1:-vps-production}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[OK]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }
log_step() { echo -e "\n${CYAN}=== $* ===${NC}"; }

# ============================================================================
# Deployment Steps
# ============================================================================

check_prerequisites() {
    log_step "Checking prerequisites"
    
    # Check SSH connectivity
    log_info "Testing SSH connection to ${VPS_HOST}..."
    if ! ssh -o ConnectTimeout=10 "$VPS_HOST" "echo 'SSH OK'" &>/dev/null; then
        log_error "Cannot connect to ${VPS_HOST}"
        exit 1
    fi
    log_success "SSH connection OK"
    
    # Check Podman
    log_info "Checking Podman on ${VPS_HOST}..."
    local podman_version
    podman_version=$(ssh "$VPS_HOST" "podman --version" 2>/dev/null || echo "NOT_INSTALLED")
    if [[ "$podman_version" == "NOT_INSTALLED" ]]; then
        log_error "Podman not installed on ${VPS_HOST}"
        exit 1
    fi
    log_success "Podman: ${podman_version}"
    
    # Check wireguard-tools for key generation
    log_info "Checking wireguard-tools..."
    if ! ssh "$VPS_HOST" "command -v wg" &>/dev/null; then
        log_warn "wireguard-tools not installed. Installing..."
        ssh "$VPS_HOST" "apt-get update && apt-get install -y wireguard-tools" || {
            log_error "Failed to install wireguard-tools"
            exit 1
        }
    fi
    log_success "wireguard-tools available"
    
    # Check jq
    log_info "Checking jq..."
    if ! ssh "$VPS_HOST" "command -v jq" &>/dev/null; then
        log_warn "jq not installed. Installing..."
        ssh "$VPS_HOST" "apt-get update && apt-get install -y jq" || {
            log_error "Failed to install jq"
            exit 1
        }
    fi
    log_success "jq available"
    
    # Check accounts file
    log_info "Checking accounts.json..."
    local account_count
    account_count=$(ssh "$VPS_HOST" "jq '.accounts | length' /var/lib/antigravity/accounts.json 2>/dev/null || echo 0")
    if [[ "$account_count" == "0" ]]; then
        log_warn "No accounts found in /var/lib/antigravity/accounts.json"
        log_warn "Deploy will generate empty configuration"
    else
        log_success "Found ${account_count} accounts"
    fi
}

deploy_scripts() {
    log_step "Deploying scripts to ${VPS_HOST}"
    
    local remote_dir="/opt/antigravity/warp-isolation"
    
    # Create directory
    ssh "$VPS_HOST" "mkdir -p ${remote_dir}"
    
    # Copy scripts
    scp "${SCRIPT_DIR}/generate-warp-containers.sh" "${VPS_HOST}:${remote_dir}/"
    scp "${SCRIPT_DIR}/verify-warp-ips.sh" "${VPS_HOST}:${remote_dir}/"
    
    # Make executable
    ssh "$VPS_HOST" "chmod +x ${remote_dir}/*.sh"
    
    log_success "Scripts deployed to ${remote_dir}"
}

create_directories() {
    log_step "Creating directories"
    
    ssh "$VPS_HOST" "
        mkdir -p /etc/antigravity/warp
        chmod 700 /etc/antigravity/warp
        
        mkdir -p /etc/containers/systemd
    "
    
    log_success "Directories created"
}

generate_containers() {
    log_step "Generating WARP containers"
    
    # Run generation script
    ssh "$VPS_HOST" "
        cd /opt/antigravity/warp-isolation
        ./generate-warp-containers.sh \
            --accounts-file /var/lib/antigravity/accounts.json \
            --quadlet-dir /etc/containers/systemd \
            --base-port 10800
    "
    
    log_success "Container quadlets generated"
}

reload_systemd() {
    log_step "Reloading systemd"
    
    ssh "$VPS_HOST" "
        systemctl daemon-reload
        
        # List generated units
        echo 'Generated WARP units:'
        systemctl list-unit-files | grep -E 'warp-|antigravity-warp' || true
    "
    
    log_success "Systemd reloaded"
}

start_containers() {
    log_step "Starting WARP containers"
    
    # Start network first
    log_info "Starting WARP network..."
    ssh "$VPS_HOST" "
        systemctl start antigravity-warp-network.service 2>/dev/null || \
        podman network create antigravity-warp --subnet 10.88.0.0/16 --gateway 10.88.0.1 2>/dev/null || \
        true
    "
    
    # Start containers
    log_info "Starting WARP containers..."
    ssh "$VPS_HOST" "
        for unit in /etc/containers/systemd/warp-*.container; do
            if [[ -f \"\$unit\" ]]; then
                name=\$(basename \"\$unit\" .container)
                echo \"Starting \${name}...\"
                systemctl start \"\${name}.service\" || echo \"Failed to start \${name}\"
            fi
        done
    "
    
    log_success "Containers started (check logs for any failures)"
}

verify_deployment() {
    log_step "Verifying deployment"
    
    # Wait for containers to initialize
    log_info "Waiting 30 seconds for VPN tunnels to establish..."
    sleep 30
    
    # Run verification
    ssh "$VPS_HOST" "
        cd /opt/antigravity/warp-isolation
        ./verify-warp-ips.sh --mapping-file /etc/antigravity/warp/ip_mapping.json
    "
}

show_status() {
    log_step "Deployment Status"
    
    ssh "$VPS_HOST" "
        echo '=== Running WARP containers ==='
        podman ps --filter 'name=warp-' --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}'
        
        echo ''
        echo '=== IP Mapping ==='
        cat /etc/antigravity/warp/ip_mapping.json | jq -r '.accounts[] | \"\(.email): port \(.warp_port)\"'
        
        echo ''
        echo '=== Integration Endpoint ==='
        echo 'Add to Antigravity config:'
        echo '  WARP_MAPPING_FILE=/etc/antigravity/warp/ip_mapping.json'
    "
}

# ============================================================================
# Main
# ============================================================================

main() {
    echo -e "${CYAN}"
    echo "╔══════════════════════════════════════════════════════════════╗"
    echo "║     Antigravity WARP Isolation - Deployment                  ║"
    echo "║     Target: ${VPS_HOST}                                      ║"
    echo "╚══════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
    
    check_prerequisites
    deploy_scripts
    create_directories
    generate_containers
    reload_systemd
    start_containers
    verify_deployment
    show_status
    
    echo ""
    log_success "Deployment complete!"
    echo ""
    echo "Next steps:"
    echo "  1. Integrate with Antigravity proxy (see README.md)"
    echo "  2. Monitor: ssh ${VPS_HOST} 'podman logs -f warp-XXXXXXXX'"
    echo "  3. Verify periodically: ssh ${VPS_HOST} '/opt/antigravity/warp-isolation/verify-warp-ips.sh'"
}

main "$@"
