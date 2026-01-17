#!/usr/bin/env bash
# ============================================================================
# Antigravity WARP Verification - IP Uniqueness Checker
# ============================================================================
# Verifies that each WARP container has a unique external IP.
# Detects IP collisions and broken proxies before they cause bans.
#
# Architecture:
#   - Fetches external IP for each container via SOCKS5 proxy
#   - Compares with host IP to detect proxy bypass
#   - Builds collision map to detect duplicate IPs
#   - Outputs JSON report for integration with Antigravity
#
# Usage:
#   ./verify-warp-ips.sh [OPTIONS]
#
# Options:
#   --mapping-file PATH    Path to ip_mapping.json (default: /etc/antigravity/warp/ip_mapping.json)
#   --output FILE          Output JSON report (default: stdout)
#   --json                 Output as JSON only (no colors)
#   --help                 Show this help
# ============================================================================

set -euo pipefail

# Default configuration
MAPPING_FILE="${MAPPING_FILE:-/etc/antigravity/warp/ip_mapping.json}"
OUTPUT_FILE="${OUTPUT_FILE:-}"
JSON_ONLY="${JSON_ONLY:-false}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

log_info() { [[ "$JSON_ONLY" == "false" ]] && echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { [[ "$JSON_ONLY" == "false" ]] && echo -e "${GREEN}[OK]${NC} $*"; }
log_warn() { [[ "$JSON_ONLY" == "false" ]] && echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { [[ "$JSON_ONLY" == "false" ]] && echo -e "${RED}[ERROR]${NC} $*" >&2; }

# ============================================================================
# IP Fetching
# ============================================================================

fetch_ip_via_socks() {
    local socks_endpoint="$1"
    local timeout="${2:-10}"
    
    # Extract host and port from socks5://host:port
    local socks_addr
    socks_addr=$(echo "$socks_endpoint" | sed 's|socks5://||')
    
    # Try multiple IP services
    local ip_services=(
        "https://api.ipify.org"
        "https://ifconfig.me/ip"
        "https://icanhazip.com"
    )
    
    for service in "${ip_services[@]}"; do
        local ip
        ip=$(curl --socks5 "$socks_addr" -sfS --max-time "$timeout" "$service" 2>/dev/null | tr -d '[:space:]')
        if [[ -n "$ip" && "$ip" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "$ip"
            return 0
        fi
    done
    
    echo "FAILED"
    return 1
}

fetch_host_ip() {
    local ip_services=(
        "https://api.ipify.org"
        "https://ifconfig.me/ip"
        "https://icanhazip.com"
    )
    
    for service in "${ip_services[@]}"; do
        local ip
        ip=$(curl -sfS --max-time 10 "$service" 2>/dev/null | tr -d '[:space:]')
        if [[ -n "$ip" && "$ip" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
            echo "$ip"
            return 0
        fi
    done
    
    echo "UNKNOWN"
    return 1
}

# ============================================================================
# Container Status Check
# ============================================================================

check_container_status() {
    local container_name="$1"
    
    if podman ps --format '{{.Names}}' | grep -q "^${container_name}$"; then
        echo "running"
    elif podman ps -a --format '{{.Names}}' | grep -q "^${container_name}$"; then
        local status
        status=$(podman inspect "$container_name" --format '{{.State.Status}}' 2>/dev/null || echo "unknown")
        echo "$status"
    else
        echo "not_found"
    fi
}

# ============================================================================
# Main Verification
# ============================================================================

verify_all_ips() {
    local mapping_file="$1"
    
    if [[ ! -f "$mapping_file" ]]; then
        log_error "Mapping file not found: ${mapping_file}"
        echo '{"error": "mapping_file_not_found", "path": "'$mapping_file'"}'
        return 1
    fi
    
    # Fetch host IP first
    log_info "Fetching host IP..."
    local host_ip
    host_ip=$(fetch_host_ip)
    log_info "Host IP: ${host_ip}"
    
    # Read accounts from mapping
    local accounts
    accounts=$(jq -c '.accounts[]' "$mapping_file")
    
    if [[ -z "$accounts" ]]; then
        log_warn "No accounts in mapping file"
        echo '{"error": "no_accounts", "path": "'$mapping_file'"}'
        return 1
    fi
    
    # Verification results
    local results=()
    local ip_to_account=()  # For collision detection
    local total=0
    local success=0
    local failed=0
    local collisions=0
    local bypasses=0
    
    while IFS= read -r account; do
        local account_id
        account_id=$(echo "$account" | jq -r '.id')
        local account_email
        account_email=$(echo "$account" | jq -r '.email')
        local warp_port
        warp_port=$(echo "$account" | jq -r '.warp_port')
        local container_name
        container_name=$(echo "$account" | jq -r '.warp_container')
        local socks_endpoint
        socks_endpoint=$(echo "$account" | jq -r '.socks5_endpoint')
        
        total=$((total + 1))
        log_info "Checking ${account_email} (${container_name}, port ${warp_port})..."
        
        # Check container status
        local container_status
        container_status=$(check_container_status "$container_name")
        
        local external_ip="FAILED"
        local status="error"
        local error_msg=""
        
        if [[ "$container_status" == "running" ]]; then
            # Fetch IP via SOCKS5
            external_ip=$(fetch_ip_via_socks "$socks_endpoint" 15)
            
            if [[ "$external_ip" == "FAILED" ]]; then
                status="proxy_failed"
                error_msg="Could not fetch IP via proxy"
                failed=$((failed + 1))
                log_error "${account_email}: Proxy failed"
            elif [[ "$external_ip" == "$host_ip" ]]; then
                status="bypass_detected"
                error_msg="Proxy IP matches host IP - VPN not working!"
                bypasses=$((bypasses + 1))
                log_error "${account_email}: PROXY BYPASS DETECTED! IP=${external_ip}"
            else
                # Check for collision
                local collision_found=false
                for entry in "${ip_to_account[@]:-}"; do
                    local existing_ip
                    existing_ip=$(echo "$entry" | cut -d'|' -f1)
                    local existing_email
                    existing_email=$(echo "$entry" | cut -d'|' -f2)
                    
                    if [[ "$existing_ip" == "$external_ip" ]]; then
                        status="collision"
                        error_msg="IP collision with ${existing_email}"
                        collisions=$((collisions + 1))
                        collision_found=true
                        log_error "${account_email}: COLLISION! Same IP as ${existing_email} (${external_ip})"
                        break
                    fi
                done
                
                if [[ "$collision_found" == "false" ]]; then
                    status="ok"
                    success=$((success + 1))
                    ip_to_account+=("${external_ip}|${account_email}")
                    log_success "${account_email}: ${external_ip}"
                fi
            fi
        else
            status="container_${container_status}"
            error_msg="Container is ${container_status}"
            failed=$((failed + 1))
            log_warn "${account_email}: Container ${container_status}"
        fi
        
        # Build result entry
        local result_json
        result_json=$(cat << EOF
{
    "account_id": "${account_id}",
    "email": "${account_email}",
    "container": "${container_name}",
    "container_status": "${container_status}",
    "socks_port": ${warp_port},
    "external_ip": "${external_ip}",
    "status": "${status}",
    "error": ${error_msg:+\"$error_msg\"}${error_msg:-null}
}
EOF
)
        results+=("$result_json")
        
    done <<< "$accounts"
    
    # Build final report
    local report
    report=$(cat << EOF
{
    "verified_at": "$(date -Iseconds)",
    "host_ip": "${host_ip}",
    "summary": {
        "total": ${total},
        "success": ${success},
        "failed": ${failed},
        "collisions": ${collisions},
        "bypasses": ${bypasses}
    },
    "accounts": [
        $(IFS=,; echo "${results[*]}")
    ]
}
EOF
)
    
    # Pretty print if jq available
    if command -v jq &>/dev/null; then
        report=$(echo "$report" | jq .)
    fi
    
    # Output
    if [[ -n "$OUTPUT_FILE" ]]; then
        echo "$report" > "$OUTPUT_FILE"
        log_success "Report saved to ${OUTPUT_FILE}"
    else
        echo "$report"
    fi
    
    # Summary
    if [[ "$JSON_ONLY" == "false" ]]; then
        echo ""
        echo -e "${CYAN}=== Verification Summary ===${NC}"
        echo -e "Total accounts: ${total}"
        echo -e "Successful: ${GREEN}${success}${NC}"
        echo -e "Failed: ${RED}${failed}${NC}"
        echo -e "Collisions: ${RED}${collisions}${NC}"
        echo -e "Bypasses: ${RED}${bypasses}${NC}"
        
        if [[ $collisions -gt 0 || $bypasses -gt 0 ]]; then
            echo ""
            log_error "CRITICAL: IP isolation is compromised!"
            return 1
        elif [[ $failed -gt 0 ]]; then
            echo ""
            log_warn "Some containers are not running. Start them with: systemctl start warp-*.service"
            return 1
        else
            echo ""
            log_success "All IPs are unique and isolated!"
            return 0
        fi
    fi
    
    # Return code based on status
    if [[ $collisions -gt 0 || $bypasses -gt 0 ]]; then
        return 2
    elif [[ $failed -gt 0 ]]; then
        return 1
    fi
    return 0
}

# ============================================================================
# CLI
# ============================================================================

show_help() {
    head -n 22 "$0" | tail -n 18
}

main() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --mapping-file)
                MAPPING_FILE="$2"
                shift 2
                ;;
            --output)
                OUTPUT_FILE="$2"
                shift 2
                ;;
            --json)
                JSON_ONLY="true"
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
    
    # Run verification
    verify_all_ips "$MAPPING_FILE"
}

main "$@"
