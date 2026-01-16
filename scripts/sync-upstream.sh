#!/usr/bin/env bash
# Vendor Overlay Sync Script
# Syncs upstream src-tauri proxy code to our crates/antigravity-core
#
# Usage: ./scripts/sync-upstream.sh [--check-only]
#
# This script maintains the Vendor Overlay architecture:
# - src-tauri/ is upstream-only (read-only reference, excluded from workspace)
# - crates/antigravity-core/src/proxy/ contains our production copy
# - Our custom logic lives in dedicated files (not synced from upstream)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

UPSTREAM_PROXY="$PROJECT_ROOT/src-tauri/src/proxy"
OUR_PROXY="$PROJECT_ROOT/crates/antigravity-core/src/proxy"

# Files/directories we maintain ourselves (never overwritten by sync)
# These are excluded from rsync --delete to prevent accidental removal
PROTECTED_FILES=(
    "mod.rs"              # Our module exports differ from upstream
    "server.rs"           # Our Axum server structure
    "token_manager.rs"    # Our adapted token_manager
    "monitor.rs"          # Our monitor implementation
    "circuit_breaker.rs"  # Our AIMD circuit breaker (upstream deleted theirs)
)

# Build rsync exclude arguments from PROTECTED_FILES
build_exclude_args() {
    local exclude_args=""
    for file in "${PROTECTED_FILES[@]}"; do
        exclude_args="$exclude_args --exclude=$file"
    done
    echo "$exclude_args"
}

EXCLUDE_ARGS=$(build_exclude_args)

CHECK_ONLY=false
if [[ "${1:-}" == "--check-only" ]]; then
    CHECK_ONLY=true
fi

echo "🔄 Antigravity Vendor Overlay Sync"
echo "   From: $UPSTREAM_PROXY"
echo "   To:   $OUR_PROXY"
echo ""

# Verify upstream exists
if [[ ! -d "$UPSTREAM_PROXY" ]]; then
    echo "❌ Error: Upstream proxy directory not found!"
    echo "   Expected: $UPSTREAM_PROXY"
    echo "   Run 'git fetch upstream' first"
    exit 1
fi

# Show upstream version
UPSTREAM_COMMIT=$(cd "$PROJECT_ROOT" && git log -1 --format="%h %s" upstream/main 2>/dev/null || echo "unknown")
echo "📌 Upstream HEAD: $UPSTREAM_COMMIT"
echo ""

if $CHECK_ONLY; then
    echo "🔍 Check-only mode: showing what would be synced"
    echo ""
fi

sync_directory() {
    local src="$1"
    local dst="$2"
    local name="$3"
    
    if [[ ! -d "$src" ]]; then
        echo "⚠️  Skipping $name (not found in upstream)"
        return
    fi
    
    echo "📦 Syncing $name..."
    if $CHECK_ONLY; then
        # shellcheck disable=SC2086
        rsync -avn --delete $EXCLUDE_ARGS "$src/" "$dst/" 2>/dev/null | grep -v "^$" | head -20 || true
    else
        # shellcheck disable=SC2086
        rsync -av --delete $EXCLUDE_ARGS "$src/" "$dst/"
    fi
}

sync_file() {
    local src="$1"
    local dst="$2"
    local name="$3"
    
    if [[ ! -f "$src" ]]; then
        echo "⚠️  Skipping $name (not found in upstream)"
        return
    fi
    
    echo "📦 Syncing $name..."
    if $CHECK_ONLY; then
        if [[ -f "$dst" ]]; then
            if ! diff -q "$src" "$dst" >/dev/null 2>&1; then
                echo "   Would update: $dst"
            else
                echo "   No changes"
            fi
        else
            echo "   Would create: $dst"
        fi
    else
        cp "$src" "$dst"
    fi
}

# 1. Sync mappers (core transformation logic - this is the gold)
sync_directory "$UPSTREAM_PROXY/mappers" "$OUR_PROXY/mappers" "mappers"

# 2. Sync common utilities
sync_directory "$UPSTREAM_PROXY/common" "$OUR_PROXY/common" "common"

# 3. Sync handlers (request handlers)
sync_directory "$UPSTREAM_PROXY/handlers" "$OUR_PROXY/handlers" "handlers"

# 4. Sync middleware
sync_directory "$UPSTREAM_PROXY/middleware" "$OUR_PROXY/middleware" "middleware"

# 5. Sync providers (z.ai, etc.)
sync_directory "$UPSTREAM_PROXY/providers" "$OUR_PROXY/providers" "providers"

# 6. Sync upstream (client)
sync_directory "$UPSTREAM_PROXY/upstream" "$OUR_PROXY/upstream" "upstream"

# 7. Sync audio
sync_directory "$UPSTREAM_PROXY/audio" "$OUR_PROXY/audio" "audio"

# 8. Sync individual files
echo ""
sync_file "$UPSTREAM_PROXY/session_manager.rs" "$OUR_PROXY/session_manager.rs" "session_manager.rs"
sync_file "$UPSTREAM_PROXY/signature_cache.rs" "$OUR_PROXY/signature_cache.rs" "signature_cache.rs"
sync_file "$UPSTREAM_PROXY/rate_limit.rs" "$OUR_PROXY/rate_limit.rs" "rate_limit.rs"
sync_file "$UPSTREAM_PROXY/project_resolver.rs" "$OUR_PROXY/project_resolver.rs" "project_resolver.rs"
sync_file "$UPSTREAM_PROXY/security.rs" "$OUR_PROXY/security.rs" "security.rs"
sync_file "$UPSTREAM_PROXY/sticky_config.rs" "$OUR_PROXY/sticky_config.rs" "sticky_config.rs"
sync_file "$UPSTREAM_PROXY/zai_vision_mcp.rs" "$OUR_PROXY/zai_vision_mcp.rs" "zai_vision_mcp.rs"
sync_file "$UPSTREAM_PROXY/zai_vision_tools.rs" "$OUR_PROXY/zai_vision_tools.rs" "zai_vision_tools.rs"

# 9. Copy token_manager as reference (we have our own implementation)
echo ""
echo "📦 Copying token_manager.rs as reference copy..."
if ! $CHECK_ONLY; then
    cp "$UPSTREAM_PROXY/token_manager.rs" "$OUR_PROXY/token_manager_upstream.rs"
fi

if $CHECK_ONLY; then
    echo ""
    echo "✅ Check complete. Run without --check-only to apply changes."
    exit 0
fi

# 10. Post-processing: Fix import paths
echo ""
echo "🔧 Fixing import paths..."

# Replace logger calls with tracing macros
# Handle both crate::modules::logger and antigravity_core::modules::logger patterns
# Key fix: remove &format!( and convert to direct tracing macro calls

# Step 1: Replace log_info/error/warn/debug patterns
for pattern in "crate::modules::logger" "antigravity_core::modules::logger"; do
    find "$OUR_PROXY" -name "*.rs" -exec sed -i \
        "s/${pattern}::log_info(\&format!/tracing::info!/g" {} \;
    find "$OUR_PROXY" -name "*.rs" -exec sed -i \
        "s/${pattern}::log_error(\&format!/tracing::error!/g" {} \;
    find "$OUR_PROXY" -name "*.rs" -exec sed -i \
        "s/${pattern}::log_warn(\&format!/tracing::warn!/g" {} \;
    find "$OUR_PROXY" -name "*.rs" -exec sed -i \
        "s/${pattern}::log_debug(\&format!/tracing::debug!/g" {} \;
done

# Step 2: Fix trailing )); which should be just ); after tracing macro conversion
# This handles the case where format!(...) was followed by ));
find "$OUR_PROXY" -name "*.rs" -exec sed -i \
    's/tracing::\(info\|error\|warn\|debug\)!(\(.*\)));/tracing::\1!(\2);/g' {} \;

# Replace use crate:: with use super:: in submodules (if needed)
# This is context-dependent, so we skip automatic replacement

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "✅ Sync complete!"
echo "   Upstream: $UPSTREAM_COMMIT"
echo ""
echo "🔍 Next steps:"
echo "   1. cargo check -p antigravity-core"
echo "   2. cargo clippy -p antigravity-core -- -D warnings"  
echo "   3. Review changes: git diff crates/antigravity-core/src/proxy"
echo "   4. git add -A && git commit -m \"feat(sync): upstream sync\""
echo "═══════════════════════════════════════════════════════════"
