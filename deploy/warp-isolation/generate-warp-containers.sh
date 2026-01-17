#!/usr/bin/env bash
# ============================================================================
# Antigravity WARP Isolation - Container Generator
# ============================================================================
# Generates isolated Gluetun/WARP containers for each Google account.
# Each account gets a unique IP via Cloudflare WARP for anti-ban protection.
#
# Architecture:
#   - Each account → warp-{account_id}.container (Gluetun + WireGuard/WARP)
#   - SOCKS5 proxy exposed on port 10800 + account_index
#   - Shared network: antigravity-warp.network
#
# Usage:
#   ./generate-warp-containers.sh [OPTIONS]
#
# Options:
#   --accounts-file PATH   Path to accounts.json (default: /var/lib/antigravity/accounts.json)
#   --quadlet-dir PATH     Quadlet output directory (default: /etc/containers/systemd)
#   --base-port PORT       Base SOCKS5 port (default: 10800)
#   --dry-run              Print generated files without writing
#   --help                 Show this help
#
# Requirements:
#   - jq
#   - Podman with systemd (quadlet)
#   - WARP credentials (generated once, stored in /etc/antigravity/warp/)
# ============================================================================

set -euo pipefail

# Default configuration
ACCOUNTS_FILE="${ACCOUNTS_FILE:-/var/lib/antigravity/accounts.json}"
QUADLET_DIR="${QUADLET_DIR:-/etc/containers/systemd}"
WARP_KEYS_DIR="${WARP_KEYS_DIR:-/etc/antigravity/warp}"
BASE_PORT="${BASE_PORT:-10800}"
DRY_RUN="${DRY_RUN:-false}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[OK]${NC} $*"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*" >&2; }

# ============================================================================
# WARP Key Generation (Direct Cloudflare API)
# ============================================================================

generate_warp_keys() {
    local account_id="$1"
    local key_dir="${WARP_KEYS_DIR}/${account_id}"
    local private_key_file="${key_dir}/private.key"
    local config_file="${key_dir}/config.json"
    
    if [[ -f "$private_key_file" && -f "$config_file" ]]; then
        log_info "WARP keys already exist for ${account_id}"
        return 0
    fi
    
    log_info "Registering WARP device for ${account_id}..."
    
    mkdir -p "$key_dir"
    chmod 700 "$key_dir"
    
    # Generate WireGuard keypair
    local private_key
    private_key=$(wg genkey)
    local public_key
    public_key=$(echo "$private_key" | wg pubkey)
    
    # Register with Cloudflare WARP API
    local device_id
    device_id=$(cat /proc/sys/kernel/random/uuid)
    
    local tos_date
    tos_date=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
    
    log_info "Registering with Cloudflare WARP API..."
    local response
    response=$(curl -sS -X POST "https://api.cloudflareclient.com/v0a2158/reg" \
        -H "Content-Type: application/json" \
        -H "CF-Client-Version: a-6.11-2223" \
        -H "User-Agent: okhttp/3.12.1" \
        -d "{
            \"key\": \"${public_key}\",
            \"install_id\": \"\",
            \"fcm_token\": \"\",
            \"tos\": \"${tos_date}\",
            \"model\": \"Linux\",
            \"serial_number\": \"${device_id}\",
            \"locale\": \"en_US\"
        }" 2>&1)
    
    # Check for error
    if echo "$response" | grep -q '"error"'; then
        log_error "WARP registration failed: $(echo "$response" | jq -r '.error // .message // .' 2>/dev/null || echo "$response")"
        return 1
    fi
    
    # Extract configuration from response
    local warp_id
    warp_id=$(echo "$response" | jq -r '.id // empty')
    
    if [[ -z "$warp_id" ]]; then
        log_error "Failed to parse WARP response: $response"
        return 1
    fi
    
    local warp_address_v4
    warp_address_v4=$(echo "$response" | jq -r '.config.interface.addresses.v4 // "172.16.0.2"')
    
    # Extract endpoint - prefer v4 IP, fallback to host
    local warp_endpoint_v4
    warp_endpoint_v4=$(echo "$response" | jq -r '.config.peers[0].endpoint.v4 // empty')
    local warp_endpoint_host
    warp_endpoint_host=$(echo "$response" | jq -r '.config.peers[0].endpoint.host // "engage.cloudflareclient.com:2408"')
    
    # Parse endpoint IP and port
    local warp_endpoint_ip
    local warp_endpoint_port
    if [[ -n "$warp_endpoint_v4" && "$warp_endpoint_v4" != "null" ]]; then
        # Format: "IP:PORT"
        warp_endpoint_ip=$(echo "$warp_endpoint_v4" | cut -d':' -f1)
        warp_endpoint_port=$(echo "$warp_endpoint_v4" | cut -d':' -f2)
        # Port might be 0, use default
        if [[ "$warp_endpoint_port" == "0" ]]; then
            warp_endpoint_port="2408"
        fi
    else
        # Fallback to known WARP IPs
        warp_endpoint_ip="162.159.192.1"
        warp_endpoint_port="2408"
    fi
    
    local warp_public_key
    warp_public_key=$(echo "$response" | jq -r '.config.peers[0].public_key // "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo="')
    
    local warp_license
    warp_license=$(echo "$response" | jq -r '.account.license // empty')
    
    local warp_token
    warp_token=$(echo "$response" | jq -r '.token // empty')
    
    # Save private key
    echo "$private_key" > "$private_key_file"
    chmod 600 "$private_key_file"
    
    # Save full config
    cat > "$config_file" << EOF
{
    "account_id": "${account_id}",
    "warp_device_id": "${warp_id}",
    "private_key": "${private_key}",
    "public_key": "${public_key}",
    "warp_public_key": "${warp_public_key}",
    "address": "${warp_address_v4}/32",
    "endpoint_ip": "${warp_endpoint_ip}",
    "endpoint_port": "${warp_endpoint_port}",
    "endpoint": "${warp_endpoint_ip}:${warp_endpoint_port}",
    "license": "${warp_license}",
    "token": "${warp_token}",
    "created_at": "$(date -Iseconds)"
}
EOF
    chmod 600 "$config_file"
    
    # Save raw response for debugging
    echo "$response" > "${key_dir}/warp_response.json"
    chmod 600 "${key_dir}/warp_response.json"
    
    log_success "Registered WARP device for ${account_id} (IP: ${warp_address_v4})"
}

# ============================================================================
# Quadlet Generation
# ============================================================================

generate_gluetun_quadlet() {
    local account_id="$1"
    local account_email="$2"
    local socks_port="$3"
    local container_name="warp-${account_id:0:8}"  # Truncate ID for readability
    
    # Read WARP config from config.json if exists
    local config_file="${WARP_KEYS_DIR}/${account_id}/config.json"
    local warp_address="172.16.0.2/32"
    local warp_endpoint_ip="162.159.192.1"
    local warp_endpoint_port="2408"
    local warp_public_key="bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo="
    
    if [[ -f "$config_file" ]]; then
        local address
        address=$(jq -r '.address // empty' "$config_file" 2>/dev/null)
        if [[ -n "$address" ]]; then
            warp_address="$address"
        fi
        
        local endpoint
        endpoint=$(jq -r '.endpoint // empty' "$config_file" 2>/dev/null)
        if [[ -n "$endpoint" && "$endpoint" != "null" ]]; then
            # Parse endpoint (format: host:port)
            warp_endpoint_ip=$(echo "$endpoint" | cut -d':' -f1)
            warp_endpoint_port=$(echo "$endpoint" | cut -d':' -f2)
        fi
        
        local public_key
        public_key=$(jq -r '.warp_public_key // empty' "$config_file" 2>/dev/null)
        if [[ -n "$public_key" && "$public_key" != "null" ]]; then
            warp_public_key="$public_key"
        fi
    fi
    
    cat << EOF
# ============================================================================
# Antigravity WARP Container - IP Isolation for Google Account
# ============================================================================
# Account: ${account_email}
# Account ID: ${account_id}
# SOCKS5 Port: ${socks_port}
# WARP Address: ${warp_address}
# Auto-generated by generate-warp-containers.sh
# ============================================================================

[Unit]
Description=WARP VPN Isolation for ${account_email}
After=network-online.target
Wants=network-online.target

[Container]
ContainerName=${container_name}
Image=ghcr.io/qdm12/gluetun:latest

# Required capabilities for VPN
AddCapability=NET_ADMIN
AddDevice=/dev/net/tun

# WARP/WireGuard configuration
Environment=VPN_SERVICE_PROVIDER=custom
Environment=VPN_TYPE=wireguard

# WireGuard addresses (from WARP registration)
Environment=WIREGUARD_ADDRESSES=${warp_address}

# Cloudflare WARP endpoint
Environment=VPN_ENDPOINT_IP=${warp_endpoint_ip}
Environment=VPN_ENDPOINT_PORT=${warp_endpoint_port}

# Cloudflare WARP public key
Environment=WIREGUARD_PUBLIC_KEY=${warp_public_key}

# Private key from secrets
Secret=warp-${account_id:0:8}-privkey,type=env,target=WIREGUARD_PRIVATE_KEY

# Firewall - allow SOCKS5 traffic only
Environment=FIREWALL_VPN_INPUT_PORTS=1080

# HTTP Proxy (Gluetun has built-in HTTP proxy, not SOCKS5!)
Environment=HTTPPROXY=on
Environment=HTTPPROXY_LISTENING_ADDRESS=:8888

# Expose SOCKS5 port for sidecar container (sidecar shares our network namespace)
PublishPort=${socks_port}:1080

# Network
Network=antigravity-warp.network

# Labels for identification
Label=antigravity.account.id=${account_id}
Label=antigravity.account.email=${account_email}
Label=antigravity.warp.port=${socks_port}

# Health check - verify VPN is working via HTTP proxy
HealthCmd=wget -q -O /dev/null --header "Host: api.ipify.org" http://127.0.0.1:8888/http://api.ipify.org || exit 1
HealthInterval=60s
HealthTimeout=15s
HealthRetries=3

[Service]
Restart=always
RestartSec=30
TimeoutStartSec=300

[Install]
WantedBy=multi-user.target default.target
EOF
}

generate_socks5_sidecar_quadlet() {
    local account_id="$1"
    local socks_port="$2"
    local gluetun_container="warp-${account_id:0:8}"
    local socks_container="${gluetun_container}-socks5"
    
    cat << EOF
# ============================================================================
# Antigravity WARP SOCKS5 Sidecar - Shares network with Gluetun VPN
# ============================================================================
# Account ID: ${account_id}
# SOCKS5 Port: ${socks_port}
# Parent container: ${gluetun_container}
# Auto-generated by generate-warp-containers.sh
# ============================================================================

[Unit]
Description=SOCKS5 Proxy Sidecar for ${gluetun_container}
After=${gluetun_container}.service
Requires=${gluetun_container}.service
BindsTo=${gluetun_container}.service

[Container]
ContainerName=${socks_container}
Image=docker.io/serjs/go-socks5-proxy:latest

# Share network namespace with Gluetun (all traffic goes through VPN!)
Network=container:${gluetun_container}

# No authentication required
Environment=REQUIRE_AUTH=false
Environment=SOCKS5_PROXY_PORT=1080

# Labels
Label=antigravity.account.id=${account_id}
Label=antigravity.socks5.port=${socks_port}
Label=antigravity.parent=${gluetun_container}

[Service]
Restart=on-failure
RestartSec=10

[Install]
WantedBy=${gluetun_container}.service
EOF
}

generate_network_quadlet() {
    cat << 'EOF'
# ============================================================================
# Antigravity WARP Network - Shared Bridge for Isolation Containers
# ============================================================================
# All WARP containers connect to this network for internal communication.
# Each container still gets external IP isolation via WARP VPN.
# Auto-generated by generate-warp-containers.sh
# ============================================================================

[Network]
NetworkName=antigravity-warp
Driver=bridge
Subnet=10.88.0.0/16
Gateway=10.88.0.1

# DNS servers (Cloudflare)
DNS=1.1.1.1
DNS=1.0.0.1

# IPv6 support
IPv6=false

[Install]
WantedBy=multi-user.target default.target
EOF
}

generate_ip_mapping_json() {
    local output_file="$1"
    shift
    local accounts=("$@")
    
    log_info "Generating IP mapping file: ${output_file}"
    
    echo "{" > "$output_file"
    echo '  "version": "1.0",' >> "$output_file"
    echo '  "generated_at": "'$(date -Iseconds)'",' >> "$output_file"
    echo '  "accounts": [' >> "$output_file"
    
    local first=true
    local port=$BASE_PORT
    for account_json in "${accounts[@]}"; do
        local account_id
        account_id=$(echo "$account_json" | jq -r '.id')
        local account_email
        account_email=$(echo "$account_json" | jq -r '.email')
        
        if [[ "$first" != "true" ]]; then
            echo "," >> "$output_file"
        fi
        first=false
        
        cat >> "$output_file" << EOF
    {
      "id": "${account_id}",
      "email": "${account_email}",
      "warp_port": ${port},
      "warp_container": "warp-${account_id:0:8}",
      "socks5_endpoint": "socks5://127.0.0.1:${port}"
    }
EOF
        port=$((port + 1))
    done
    
    echo "  ]" >> "$output_file"
    echo "}" >> "$output_file"
    
    log_success "IP mapping saved to ${output_file}"
}

# ============================================================================
# Podman Secret Management
# ============================================================================

create_podman_secret() {
    local account_id="$1"
    local secret_name="warp-${account_id:0:8}-privkey"
    local key_file="${WARP_KEYS_DIR}/${account_id}/private.key"
    
    if ! podman secret exists "$secret_name" 2>/dev/null; then
        if [[ -f "$key_file" ]]; then
            log_info "Creating Podman secret: ${secret_name}"
            podman secret create "$secret_name" "$key_file"
            log_success "Secret ${secret_name} created"
        else
            log_error "Private key not found: ${key_file}"
            return 1
        fi
    else
        log_info "Secret ${secret_name} already exists"
    fi
}

# ============================================================================
# Main Logic
# ============================================================================

show_help() {
    head -n 30 "$0" | tail -n 25
}

main() {
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --accounts-file)
                ACCOUNTS_FILE="$2"
                shift 2
                ;;
            --quadlet-dir)
                QUADLET_DIR="$2"
                shift 2
                ;;
            --base-port)
                BASE_PORT="$2"
                shift 2
                ;;
            --dry-run)
                DRY_RUN="true"
                shift
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    # Validate prerequisites
    if ! command -v jq &>/dev/null; then
        log_error "jq is required but not installed"
        exit 1
    fi
    
    if ! command -v podman &>/dev/null; then
        log_error "podman is required but not installed"
        exit 1
    fi
    
    if [[ ! -f "$ACCOUNTS_FILE" ]]; then
        log_error "Accounts file not found: ${ACCOUNTS_FILE}"
        exit 1
    fi
    
    # Read accounts
    local accounts_json
    accounts_json=$(jq -c '.accounts[]' "$ACCOUNTS_FILE")
    
    if [[ -z "$accounts_json" ]]; then
        log_warn "No accounts found in ${ACCOUNTS_FILE}"
        exit 0
    fi
    
    local account_count
    account_count=$(echo "$accounts_json" | wc -l)
    log_info "Found ${account_count} accounts in ${ACCOUNTS_FILE}"
    
    # Create directories
    if [[ "$DRY_RUN" != "true" ]]; then
        mkdir -p "$QUADLET_DIR"
        mkdir -p "$WARP_KEYS_DIR"
    fi
    
    # Generate network quadlet (once)
    local network_file="${QUADLET_DIR}/antigravity-warp.network"
    log_info "Generating network quadlet: ${network_file}"
    if [[ "$DRY_RUN" == "true" ]]; then
        echo "=== antigravity-warp.network ==="
        generate_network_quadlet
        echo ""
    else
        if [[ ! -f "$network_file" ]]; then
            generate_network_quadlet > "$network_file"
            log_success "Created ${network_file}"
        else
            log_info "Network quadlet already exists: ${network_file}"
        fi
    fi
    
    # Generate container quadlets for each account
    local port=$BASE_PORT
    local account_array=()
    
    while IFS= read -r account_json; do
        local account_id
        account_id=$(echo "$account_json" | jq -r '.id')
        local account_email
        account_email=$(echo "$account_json" | jq -r '.email')
        
        account_array+=("$account_json")
        
        local container_name="warp-${account_id:0:8}"
        local socks_container="${container_name}-socks5"
        local gluetun_quadlet="${QUADLET_DIR}/${container_name}.container"
        local socks_quadlet="${QUADLET_DIR}/${socks_container}.container"
        
        log_info "Processing account: ${account_email} (port: ${port})"
        
        if [[ "$DRY_RUN" == "true" ]]; then
            echo "=== ${container_name}.container ==="
            generate_gluetun_quadlet "$account_id" "$account_email" "$port"
            echo ""
            echo "=== ${socks_container}.container ==="
            generate_socks5_sidecar_quadlet "$account_id" "$port"
            echo ""
        else
            # Generate WARP keys if needed
            generate_warp_keys "$account_id"
            
            # Create Podman secret
            create_podman_secret "$account_id"
            
            # Generate Gluetun quadlet
            generate_gluetun_quadlet "$account_id" "$account_email" "$port" > "$gluetun_quadlet"
            log_success "Created ${gluetun_quadlet}"
            
            # Generate SOCKS5 sidecar quadlet
            generate_socks5_sidecar_quadlet "$account_id" "$port" > "$socks_quadlet"
            log_success "Created ${socks_quadlet}"
        fi
        
        port=$((port + 1))
    done <<< "$accounts_json"
    
    # Generate IP mapping file
    local mapping_file="${WARP_KEYS_DIR}/ip_mapping.json"
    if [[ "$DRY_RUN" != "true" ]]; then
        generate_ip_mapping_json "$mapping_file" "${account_array[@]}"
    fi
    
    # Summary
    echo ""
    log_success "=== Generation Complete ==="
    log_info "Total accounts: ${account_count}"
    log_info "Port range: ${BASE_PORT} - $((BASE_PORT + account_count - 1))"
    log_info "Quadlet directory: ${QUADLET_DIR}"
    log_info "WARP keys directory: ${WARP_KEYS_DIR}"
    
    if [[ "$DRY_RUN" != "true" ]]; then
        echo ""
        log_info "Next steps:"
        echo "  1. Reload systemd: systemctl daemon-reload"
        echo "  2. Start network:  systemctl start antigravity-warp-network.service"
        echo "  3. Start containers: systemctl start warp-*.service"
        echo "  4. Verify IPs: ./verify-warp-ips.sh"
    fi
}

main "$@"
