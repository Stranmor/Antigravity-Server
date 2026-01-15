#!/usr/bin/env bash
# Vendor Overlay Sync Script
# Syncs upstream src-tauri proxy code to our crates/antigravity-core
#
# Usage: ./scripts/sync-upstream.sh
#
# This script maintains the Vendor Overlay architecture:
# - src-tauri/ is upstream-only (read-only reference)
# - crates/antigravity-core/src/proxy/ contains our adapted copy
# - Our custom logic lives in files prefixed with our_ or aimd_

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

UPSTREAM_PROXY="$PROJECT_ROOT/src-tauri/src/proxy"
OUR_PROXY="$PROJECT_ROOT/crates/antigravity-core/src/proxy"

echo "🔄 Syncing upstream proxy code..."
echo "   From: $UPSTREAM_PROXY"
echo "   To:   $OUR_PROXY"

# 1. Sync mappers (core transformation logic)
echo ""
echo "📦 Syncing mappers..."
rsync -av --delete \
    "$UPSTREAM_PROXY/mappers/" \
    "$OUR_PROXY/mappers/"

# 2. Sync session_manager (session tracking)
echo ""
echo "📦 Syncing session_manager.rs..."
cp "$UPSTREAM_PROXY/session_manager.rs" "$OUR_PROXY/"

# 3. Sync signature_cache (thinking signatures)
echo ""
echo "📦 Syncing signature_cache.rs..."
cp "$UPSTREAM_PROXY/signature_cache.rs" "$OUR_PROXY/"

# 4. Copy token_manager as reference (we have our own implementation)
echo ""
echo "📦 Copying token_manager.rs as reference..."
cp "$UPSTREAM_PROXY/token_manager.rs" "$OUR_PROXY/token_manager_upstream.rs"

# 5. Sync common utilities
echo ""
echo "📦 Syncing common/..."
rsync -av --delete \
    "$UPSTREAM_PROXY/common/" \
    "$OUR_PROXY/common/"

# 6. Sync handlers (request handlers)
echo ""
echo "📦 Syncing handlers/..."
rsync -av --delete \
    "$UPSTREAM_PROXY/handlers/" \
    "$OUR_PROXY/handlers/"

# 7. Post-processing: Fix import paths
echo ""
echo "🔧 Fixing import paths..."

# Replace antigravity_core::modules::logger with tracing
find "$OUR_PROXY" -name "*.rs" -exec sed -i \
    's/antigravity_core::modules::logger::log_info(\&format!/tracing::info!/g' {} \;
find "$OUR_PROXY" -name "*.rs" -exec sed -i \
    's/antigravity_core::modules::logger::log_error(\&format!/tracing::error!/g' {} \;
find "$OUR_PROXY" -name "*.rs" -exec sed -i \
    's/antigravity_core::modules::logger::log_warn(\&format!/tracing::warn!/g' {} \;
find "$OUR_PROXY" -name "*.rs" -exec sed -i \
    's/antigravity_core::modules::logger::log_debug(\&format!/tracing::debug!/g' {} \;

# Fix common pattern where sed leaves broken ))
find "$OUR_PROXY" -name "*.rs" -exec sed -i \
    's/tracing::info!\($/tracing::info!(/g' {} \;

# 8. Get last upstream commit for tracking
UPSTREAM_COMMIT=$(cd "$PROJECT_ROOT" && git log -1 --format="%H %s" upstream/main 2>/dev/null || echo "unknown")
echo ""
echo "✅ Sync complete!"
echo "   Upstream commit: $UPSTREAM_COMMIT"
echo ""
echo "⚠️  Remember to:"
echo "   1. Run 'cargo check -p antigravity-core' to verify"
echo "   2. Review clippy warnings"
echo "   3. Commit the changes"

