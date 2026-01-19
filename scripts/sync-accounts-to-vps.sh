#!/usr/bin/env bash
# Sync local Antigravity accounts to VPS
# Usage: ./sync-accounts-to-vps.sh [--reload]
set -euo pipefail

VPS_HOST="${VPS_HOST:-vps-production}"
LOCAL_ACCOUNTS_DIR="${HOME}/.antigravity_tools/accounts"
LOCAL_INDEX="${HOME}/.antigravity_tools/accounts.json"
VPS_DATA_DIR="/var/lib/antigravity"

log() { echo "[$(date '+%H:%M:%S')] $*"; }
error() { echo "[$(date '+%H:%M:%S')] ❌ $*" >&2; exit 1; }

# Check local files exist
[[ -f "$LOCAL_INDEX" ]] || error "Local accounts.json not found at $LOCAL_INDEX"
[[ -d "$LOCAL_ACCOUNTS_DIR" ]] || error "Local accounts dir not found at $LOCAL_ACCOUNTS_DIR"

# Get list of active accounts from index
ACCOUNT_IDS=$(jq -r '.accounts[].id' "$LOCAL_INDEX")
ACCOUNT_COUNT=$(echo "$ACCOUNT_IDS" | wc -l)

log "📦 Found $ACCOUNT_COUNT accounts to sync"

# Sync accounts.json index
log "📤 Syncing accounts.json..."
scp -q "$LOCAL_INDEX" "${VPS_HOST}:${VPS_DATA_DIR}/accounts.json"

# Sync each account file
for ACCOUNT_ID in $ACCOUNT_IDS; do
    ACCOUNT_FILE="${LOCAL_ACCOUNTS_DIR}/${ACCOUNT_ID}.json"
    if [[ -f "$ACCOUNT_FILE" ]]; then
        EMAIL=$(jq -r '.email // .account.email // "unknown"' "$ACCOUNT_FILE" 2>/dev/null || echo "unknown")
        log "  📤 ${EMAIL} (${ACCOUNT_ID:0:8}...)"
        scp -q "$ACCOUNT_FILE" "${VPS_HOST}:${VPS_DATA_DIR}/accounts/"
    else
        log "  ⚠️  Missing file for ${ACCOUNT_ID:0:8}..."
    fi
done

# Clean up orphaned accounts on VPS
log "🧹 Cleaning orphaned accounts on VPS..."
VALID_IDS_ONELINE=$(echo "$ACCOUNT_IDS" | tr '\n' ' ')
ssh "$VPS_HOST" "
ACCOUNTS_DIR='/var/lib/antigravity/accounts'
VALID_IDS='$VALID_IDS_ONELINE'

for FILE in \"\$ACCOUNTS_DIR\"/*.json; do
    [ -f \"\$FILE\" ] || continue
    BASENAME=\$(basename \"\$FILE\" .json)
    if ! echo \"\$VALID_IDS\" | grep -q \"\$BASENAME\"; then
        echo \"  Removing orphan: \$BASENAME\"
        rm -f \"\$FILE\"
    fi
done
"

log "✅ Accounts synced to VPS"

# Reload server if requested
if [[ "${1:-}" == "--reload" ]]; then
    log "🔄 Reloading antigravity-server..."
    ssh "$VPS_HOST" "systemctl restart antigravity-server"
    sleep 2
    
    # Verify
    HEALTH=$(ssh "$VPS_HOST" "curl -s http://127.0.0.1:8045/healthz" 2>/dev/null || echo "failed")
    if [[ "$HEALTH" == *"ok"* ]]; then
        log "✅ Server reloaded and healthy"
    else
        log "⚠️  Server may have issues: $HEALTH"
    fi
fi

log "📊 VPS account status:"
ssh "$VPS_HOST" "ls -la ${VPS_DATA_DIR}/accounts/*.json 2>/dev/null | wc -l" | xargs -I{} echo "   {} account files on VPS"
