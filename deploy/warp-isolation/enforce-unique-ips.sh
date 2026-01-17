#!/usr/bin/env bash
# ============================================================================
# Antigravity WARP IP Uniqueness Enforcer
# ============================================================================
# Ensures each account has a UNIQUE external IP.
# Re-registers WARP devices until all IPs are unique.
#
# CRITICAL: This is a STRICT requirement - no duplicate IPs allowed!
#
# Usage:
#   ./enforce-unique-ips.sh [OPTIONS]
#
# Options:
#   --mapping-file PATH   Path to ip_mapping.json
#   --max-retries N       Max retries per account (default: 10)
#   --help                Show this help
# ============================================================================

set -euo pipefail

# Configuration
MAPPING_FILE="${MAPPING_FILE:-/etc/antigravity/warp/ip_mapping.json}"
WARP_KEYS_DIR="${WARP_KEYS_DIR:-/etc/antigravity/warp}"
MAX_RETRIES="${MAX_RETRIES:-10}"

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

# ============================================================================
# IP Fetching
# ============================================================================

fetch_ip_via_socks() {
    local port="$1"
    local timeout="${2:-15}"
    
    for service in "https://api.ipify.org" "https://ifconfig.me/ip" "https://icanhazip.com"; do
        local ip
        ip=$(curl --socks5 "127.0.0.1:${port}" -sfS --max-time "$timeout" "$service" 2>/dev/null | tr -d '[:space:]')
        if [[ -n "$ip" && "$ip" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "$ip"
            return 0
        fi
    done
    
    echo "FAILED"
    return 1
}

# ============================================================================
# WARP Re-registration
# ============================================================================

reregister_warp_device() {
    local account_id="$1"
    local short_id="${account_id:0:8}"
    local key_dir="${WARP_KEYS_DIR}/${account_id}"
    
    log_info "Re-registering WARP device for ${short_id}..."
    
    # Generate new WireGuard keypair
    local private_key
    private_key=$(wg genkey)
    local public_key
    public_key=$(echo "$private_key" | wg pubkey)
    
    # New device ID for fresh registration
    local device_id
    device_id=$(cat /proc/sys/kernel/random/uuid)
    
    local tos_date
    tos_date=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
    
    # Register with Cloudflare WARP API
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
        log_error "WARP registration failed"
        return 1
    fi
    
    # Extract configuration
    local warp_id
    warp_id=$(echo "$response" | jq -r '.id // empty')
    
    if [[ -z "$warp_id" ]]; then
        log_error "Failed to parse WARP response"
        return 1
    fi
    
    local warp_address
    warp_address=$(echo "$response" | jq -r '.config.interface.addresses.v4 // "172.16.0.2"')
    
    local warp_endpoint_v4
    warp_endpoint_v4=$(echo "$response" | jq -r '.config.peers[0].endpoint.v4 // empty')
    
    local warp_public_key
    warp_public_key=$(echo "$response" | jq -r '.config.peers[0].public_key // "bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo="')
    
    # Parse endpoint
    local endpoint_ip endpoint_port
    if [[ -n "$warp_endpoint_v4" && "$warp_endpoint_v4" != "null" ]]; then
        endpoint_ip=$(echo "$warp_endpoint_v4" | cut -d':' -f1)
        endpoint_port=$(echo "$warp_endpoint_v4" | cut -d':' -f2)
        [[ "$endpoint_port" == "0" ]] && endpoint_port="2408"
    else
        endpoint_ip="162.159.192.1"
        endpoint_port="2408"
    fi
    
    # Save new key
    echo "$private_key" > "${key_dir}/private.key"
    chmod 600 "${key_dir}/private.key"
    
    # Update config.json
    cat > "${key_dir}/config.json" << EOF
{
    "account_id": "${account_id}",
    "warp_device_id": "${warp_id}",
    "private_key": "${private_key}",
    "public_key": "${public_key}",
    "warp_public_key": "${warp_public_key}",
    "address": "${warp_address}/32",
    "endpoint_ip": "${endpoint_ip}",
    "endpoint_port": "${endpoint_port}",
    "endpoint": "${endpoint_ip}:${endpoint_port}",
    "created_at": "$(date -Iseconds)"
}
EOF
    chmod 600 "${key_dir}/config.json"
    
    # Update Podman secret
    local secret_name="warp-${short_id}-privkey"
    podman secret rm "$secret_name" 2>/dev/null || true
    podman secret create "$secret_name" "${key_dir}/private.key"
    
    # Restart containers
    systemctl restart "warp-${short_id}.service"
    sleep 5
    systemctl restart "warp-${short_id}-socks5.service"
    sleep 3
    
    return 0
}

# ============================================================================
# Main Enforcement Logic
# ============================================================================

enforce_unique_ips() {
    if [[ ! -f "$MAPPING_FILE" ]]; then
        log_error "Mapping file not found: ${MAPPING_FILE}"
        exit 1
    fi
    
    log_info "Loading account mapping from ${MAPPING_FILE}..."
    
    # Build initial IP map
    declare -A ip_to_account
    declare -A account_to_port
    declare -A account_to_ip
    local accounts=()
    
    while IFS= read -r account; do
        local account_id
        account_id=$(echo "$account" | jq -r '.id')
        local port
        port=$(echo "$account" | jq -r '.warp_port')
        
        accounts+=("$account_id")
        account_to_port["$account_id"]="$port"
    done < <(jq -c '.accounts[]' "$MAPPING_FILE")
    
    log_info "Found ${#accounts[@]} accounts"
    
    # Initial IP scan
    echo ""
    log_info "=== Initial IP Scan ==="
    for account_id in "${accounts[@]}"; do
        local port="${account_to_port[$account_id]}"
        local ip
        ip=$(fetch_ip_via_socks "$port")
        account_to_ip["$account_id"]="$ip"
        
        if [[ "$ip" != "FAILED" ]]; then
            if [[ -n "${ip_to_account[$ip]:-}" ]]; then
                log_warn "COLLISION: ${account_id:0:8} (port $port) → $ip (same as ${ip_to_account[$ip]:0:8})"
            else
                log_success "${account_id:0:8} (port $port) → $ip"
                ip_to_account["$ip"]="$account_id"
            fi
        else
            log_error "${account_id:0:8} (port $port) → FAILED"
        fi
    done
    
    # Find and fix collisions
    echo ""
    log_info "=== Enforcing IP Uniqueness ==="
    
    local total_retries=0
    local max_total_retries=$((MAX_RETRIES * ${#accounts[@]}))
    
    while true; do
        # Rebuild IP map
        unset ip_to_account
        declare -A ip_to_account
        local collisions=()
        
        for account_id in "${accounts[@]}"; do
            local ip="${account_to_ip[$account_id]}"
            
            if [[ "$ip" == "FAILED" ]]; then
                collisions+=("$account_id")
            elif [[ -n "${ip_to_account[$ip]:-}" ]]; then
                # This account has a duplicate IP
                collisions+=("$account_id")
            else
                ip_to_account["$ip"]="$account_id"
            fi
        done
        
        if [[ ${#collisions[@]} -eq 0 ]]; then
            break
        fi
        
        if [[ $total_retries -ge $max_total_retries ]]; then
            log_error "Max retries reached. Could not achieve unique IPs for all accounts."
            log_error "This is likely a Cloudflare WARP Free limitation."
            log_error "Consider using WARP+ or alternative proxies for remaining accounts."
            break
        fi
        
        # Fix first collision
        local problem_account="${collisions[0]}"
        local problem_port="${account_to_port[$problem_account]}"
        local old_ip="${account_to_ip[$problem_account]}"
        
        log_warn "Fixing collision for ${problem_account:0:8} (current IP: $old_ip)"
        
        # Re-register
        if reregister_warp_device "$problem_account"; then
            # Wait and get new IP
            sleep 5
            local new_ip
            new_ip=$(fetch_ip_via_socks "$problem_port")
            account_to_ip["$problem_account"]="$new_ip"
            
            if [[ "$new_ip" != "$old_ip" && "$new_ip" != "FAILED" ]]; then
                # Check if new IP is also a collision
                local is_collision=false
                for existing_ip in "${!ip_to_account[@]}"; do
                    if [[ "$existing_ip" == "$new_ip" ]]; then
                        is_collision=true
                        break
                    fi
                done
                
                if [[ "$is_collision" == "false" ]]; then
                    log_success "${problem_account:0:8}: $old_ip → $new_ip (UNIQUE!)"
                else
                    log_warn "${problem_account:0:8}: $old_ip → $new_ip (still collision, retrying...)"
                fi
            else
                log_warn "${problem_account:0:8}: IP unchanged or failed ($new_ip)"
            fi
        else
            log_error "Re-registration failed for ${problem_account:0:8}"
        fi
        
        total_retries=$((total_retries + 1))
        
        # Short delay between retries
        sleep 2
    done
    
    # Final report
    echo ""
    log_info "=== Final IP Distribution ==="
    unset ip_to_account
    declare -A ip_to_account
    local unique_count=0
    local failed_count=0
    
    for account_id in "${accounts[@]}"; do
        local port="${account_to_port[$account_id]}"
        local ip
        ip=$(fetch_ip_via_socks "$port")
        
        if [[ "$ip" == "FAILED" ]]; then
            log_error "${account_id:0:8} (port $port) → FAILED"
            failed_count=$((failed_count + 1))
        elif [[ -n "${ip_to_account[$ip]:-}" ]]; then
            log_error "${account_id:0:8} (port $port) → $ip (COLLISION with ${ip_to_account[$ip]:0:8})"
        else
            log_success "${account_id:0:8} (port $port) → $ip"
            ip_to_account["$ip"]="$account_id"
            unique_count=$((unique_count + 1))
        fi
    done
    
    echo ""
    echo -e "${CYAN}=== Summary ===${NC}"
    echo "Total accounts: ${#accounts[@]}"
    echo "Unique IPs: ${unique_count}"
    echo "Collisions: $((${#accounts[@]} - unique_count - failed_count))"
    echo "Failed: ${failed_count}"
    
    if [[ $unique_count -eq ${#accounts[@]} ]]; then
        log_success "All accounts have UNIQUE IPs!"
        return 0
    else
        log_error "IP uniqueness NOT achieved. Consider alternative solutions."
        return 1
    fi
}

# ============================================================================
# CLI
# ============================================================================

show_help() {
    head -n 18 "$0" | tail -n 14
}

main() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --mapping-file)
                MAPPING_FILE="$2"
                shift 2
                ;;
            --max-retries)
                MAX_RETRIES="$2"
                shift 2
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
    
    echo -e "${CYAN}"
    echo "╔══════════════════════════════════════════════════════════════╗"
    echo "║     Antigravity WARP IP Uniqueness Enforcer                  ║"
    echo "║     STRICT MODE: No duplicate IPs allowed                    ║"
    echo "╚══════════════════════════════════════════════════════════════╝"
    echo -e "${NC}"
    
    enforce_unique_ips
}

main "$@"
