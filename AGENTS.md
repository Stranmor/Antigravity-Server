# Antigravity Manager - Project Status

## 🚨 ARCHITECTURAL DECISION [2026-01-12]: Tauri → Headless Daemon + WebUI

**Problem:** Tauri/WebKitGTK renders black screen on target system. 2 days of debugging failed.

**Root Cause:** GTK WebView is fragile on Linux (especially Nvidia/Wayland). Desktop apps are not our core problem.

**Solution (SOTA Model: Syncthing/Transmission):**

```
┌─────────────────────────────────────────────────────┐
│  antigravity-server (Rust Daemon)                   │
│  ├── Proxy Logic (accounts, rotation, handlers)    │
│  ├── HTTP API (REST/JSON-RPC for CLI/UI)           │
│  └── Static File Server (serves Leptos dist/)      │
│                                                     │
│  Accessible via: http://localhost:8045              │
└─────────────────────────────────────────────────────┘
         ↑                    ↑
         │                    │
┌────────┴───────┐    ┌───────┴────────┐
│  Browser UI    │    │  CLI (ag)      │
│  (Chrome/FF)   │    │  HTTP client   │
│  Leptos WASM   │    │  curl wrapper  │
└────────────────┘    └────────────────┘
```

**Benefits:**
- ✅ No Tauri, no GTK, no WebKit
- ✅ Browser rendering is 100% reliable
- ✅ CLI for automation
- ✅ Leptos UI reused as-is

**Migration Tasks:**
- [x] Create `antigravity-server` binary (Axum + static files + proxy logic) ✅
- [ ] Move proxy handlers from `src-tauri` to `antigravity-server`
- [x] Add REST API endpoints matching Tauri IPC commands ✅
- [x] Serve `src-leptos/dist/` as static files ✅
- [x] Update systemd service to run `antigravity-server` ✅
- [x] Frontend uses HTTP API instead of Tauri IPC ✅
- [ ] Create `ag` CLI that calls HTTP API
- [ ] Integrate proxy logic from `antigravity-core`
- [ ] Delete `src-tauri/` after verification

---

## Current Status: Architecture Migration Complete (Core)

**UI Status:** Leptos WebUI works in browser ✅
**Backend Status:** antigravity-server serving API + static files ✅
**Build Status:** `just build-server` produces working binary ✅
**Verified:** Dashboard shows 5 accounts, quotas, personalized greeting

## Completed Features

### Core Types & IPC ✅
- Account, ProxyStatus, AppConfig types
- Full Tauri IPC bindings (OAuth, quotas, logs, updates)
- ProxyConfig, RefreshStats, ProxyStats types
- Display/FromStr implementations for ProxyAuthMode, ZaiDispatchMode

### Dashboard ✅
- Personalized greeting with account name
- 5 stats cards (Total, Gemini Quota, Image Quota, Claude Quota, Low Quota)
- Subtitle hints on cards (Sufficient/Low)
- Current account section with quota bars  
- Best accounts list (top 5 by quota)
- Tier breakdown (Ultra/Pro/Free/Low)
- Quick action cards
- Export to clipboard (JSON format)

### Accounts Page ✅
- View modes: List (table) and Grid (cards)
- Filter tabs: All / Pro / Ultra / Free with counts
- Search by email
- Full pagination with page size selector
- Individual account actions: switch, refresh, delete
- Toggle proxy status per account
- Batch delete selected accounts
- Confirmation modals for all destructive actions
- OAuth login button
- Sync from local DB button

### API Proxy Page ✅
- Start/Stop proxy with status indicator
- Configuration: Port, Timeout, Auto-start, Logging
- Access control: LAN access, Auth mode selector
- API key: display, copy, regenerate
- Model routing section (collapsible):
  - Add custom mappings
  - Apply presets (GPT-4→Gemini etc)
  - Reset all mappings
- Scheduling section (collapsible):
  - Mode selector (Balance/Priority/Sticky)
  - Sticky session TTL
  - Clear session bindings
- [x] Resolve merge conflicts in `Antigravity-Manager` [2026-01-12]
- [x] Implement "Upstream Isolation" (Option 4) for handlers [2026-01-12]
  - [x] Create `handlers/custom/`
  - [x] Feature-gate custom logic
- [x] Restore Leptos UI from git reflog [2026-01-12]
- [x] Create `Justfile` for build/sync automation [2026-01-12]
- [ ] Build & Install local version (In Progress)
- [ ] Verify UI functionality (Green/Teal Theme) bindings
- Z.ai Provider section (collapsible):
  - Enable/disable toggle
  - Base URL, API Key configuration
  - Dispatch mode selector
  - Model mapping
- Quick start section:
  - Protocol tabs (OpenAI/Anthropic/Gemini)
  - Model selector dropdown
  - Dynamic Python code examples
  - Base URL with copy button

### Settings Page ✅
- Language/Theme selection
- Auto-launch toggle
- Quota refresh settings
- Check for updates button
- Clear logs button
- Open data folder button

### Monitor Page ✅
- Real-time proxy request logs
- Auto-refresh (2s interval)
- Request statistics
- Filter by status/model

## Shared Components ✅
- Modal (Confirm/Alert/Danger types)
- Pagination (with page size selector)
- AccountCard (for grid view)
- StatsCard (with optional subtitle)
- Button (variants: Primary/Secondary/Danger/Ghost)
- CollapsibleCard (expandable sections)
- Select (custom dropdown with search)

## Build Configuration

### wasm-opt with bulk memory support ✅
- **Issue**: Rust 1.82+ generates bulk memory operations by default  
- **Solution**: Added `data-wasm-opt-params="--enable-bulk-memory --enable-nontrapping-float-to-int"` in index.html
- **Optimization**: Using `data-wasm-opt="z"` for maximum size reduction
- **Result**: Release build works, WASM bundle ~1.8MB optimized

## Architecture Notes
- Leptos 0.7 with CSR mode
- Centralized AppState via Context
- Type-safe Tauri IPC bindings
- Reactive signals and memos throughout
- Component-based UI design
- ChildrenFn pattern for Show components

## Verification

**Browser Test (2026-01-11 18:55):**
- ✅ Dashboard renders with stats cards and quick actions
- ✅ Accounts page shows table/grid with filters
- ✅ API Proxy page shows configuration, routing, scheduling
- ✅ Settings page shows all preferences
- ✅ Navigation sidebar works correctly

## CI/CD Pipeline ✅

### Release Workflow (`.github/workflows/release.yml`)
2-stage optimized pipeline:

```
Stage 1: Build Frontend (WASM)
├── Cache Trunk binary
├── Cache Rust dependencies (shared-key: leptos-wasm)
├── trunk build --release
└── Upload dist/ as artifact

Stage 2: Build Native (parallel per platform)
├── Download frontend artifact
├── Cache Rust dependencies (per-platform keys)
└── tauri-apps/tauri-action → GitHub Release
    ├── macOS (ARM64, x64, Universal)
    ├── Linux (x64, ARM64) → .deb, .rpm, .AppImage
    └── Windows (x64) → .msi, .exe

Stage 3: Cleanup
└── Delete temporary artifacts
```

**Triggers:** `v*` tags, manual workflow_dispatch

### CI Workflow (`.github/workflows/ci.yml`)
Parallel quality checks on every PR/push:
- **lint**: rustfmt + clippy (src-tauri + src-leptos)
- **build-check**: cargo check for all packages
- **test**: cargo test for src-tauri
- **build-frontend**: trunk build verification

### Dependabot (`.github/dependabot.yml`)
- Weekly Cargo updates (minor/patch grouped)
- Weekly GitHub Actions updates

### Local Build Commands (`justfile`)
```bash
just dev          # Start development server
just build        # Production build
just lint         # Clippy checks
just test         # Run tests
just frontend-release  # Build Leptos only
just build-deb    # Linux .deb package
```

## 2026-01-11 - Architectural Refactoring (Backend)
- **Status:** Completed ✅
- **Action:** Migrated core business logic from `src-tauri` to `antigravity-core`.
- **Scope:**
  - Moved `account`, `config`, `logger`, `process`, `quota`, `oauth`, `migration` modules to `core`.
  - Renamed `db.rs` to `vscode.rs` in `core` (VSCode token injection logic).
  - Consolidated `account.rs` logic (CRUD + Switch/Quota logic).
  - Updated `src-tauri` commands to use `antigravity_core` modules.
  - Removed duplicated code in `src-tauri`.
- **Benefit:** 
  - Clean Architecture: Business logic is now UI-agnostic.
  - Better testability of core modules.
  - Reduced coupling between Tauri and Core.
- **Verification:** `cargo check --workspace` passes.

## 2026-01-12 - Restored AIMD Predictive Rate Limiting System

**Status:** Completed ✅
**Issue:** System was accidentally deleted during Tauri → Headless migration (commit 7279853e)

### Restored Modules

```
src-tauri/src/proxy/
├── adaptive_limit.rs   # AIMD controller + per-account trackers
├── smart_prober.rs     # Speculative hedging strategies  
├── health.rs           # Account health monitoring + auto-recovery
├── prometheus.rs       # Prometheus metrics for observability
├── common/
│   └── circuit_breaker.rs  # Fast-fail pattern for failing accounts
└── handlers/
    └── helpers.rs      # Integration functions for handlers
```

### How It Works

**AIMD (Additive Increase, Multiplicative Decrease):**
- Tracks *confirmed limit* per account (requests/minute)
- **Working threshold** = 85% of confirmed limit (safety margin)
- On sustained success above threshold: +5% limit expansion
- On 429 error: ×0.7 limit contraction (multiplicative decrease)

**Probe Strategies (based on usage ratio):**
- `< 70%` → **None**: Normal operation
- `70-85%` → **CheapProbe**: Fire background 1-token request
- `85-95%` → **DelayedHedge**: Secondary request after P95 latency
- `> 95%` → **ImmediateHedge**: Parallel request immediately

**Circuit Breaker States:**
- **Closed**: Normal operation, requests pass through
- **Open**: Account failing, requests fail-fast (60s timeout)
- **HalfOpen**: Testing recovery with limited requests

### Integration Points

Handlers call into `AppState` fields:
```rust
state.adaptive_limits.usage_ratio(account_id)  // Get current usage
state.smart_prober.should_allow(account_id)    // Check if allowed
state.health_monitor.record_error(...)         // Track failures
state.circuit_breaker.should_allow(...)        // Fast-fail check
```

### Metrics (Prometheus)

- `antigravity_adaptive_probes_total{strategy}` - Probes by strategy
- `antigravity_aimd_rewards_total` - Limit expansions
- `antigravity_aimd_penalties_total` - Limit contractions
- `antigravity_hedge_wins_total` - Hedge request wins

## 2026-01-15 - Vendor Overlay Architecture for Upstream Sync

**Status:** Implemented ✅
**Commit:** e78dfea5

### Problem

Direct `git merge upstream/main` creates 50+ merge conflicts because:
- Upstream is Tauri desktop app with React UI
- Our fork is headless Axum server with Leptos UI
- Every merge requires tedious conflict resolution, risking loss of custom logic

### Solution: Vendor Overlay Pattern

```
Antigravity-Manager/
├── src-tauri/                    # ← UPSTREAM ONLY (read-only reference)
│   └── src/proxy/                # Their code — never edit directly
│       ├── mappers/claude/       # Bugfixes source
│       └── token_manager.rs      # Reference implementation
│
├── antigravity-server/           # ← OUR HEADLESS IMPLEMENTATION
│   └── src/
│       └── proxy/                # Our handlers (import from core)
│
├── crates/antigravity-core/      # ← CORE LOGIC (ported from src-tauri)
│   └── src/proxy/
│       ├── mappers/              # SYNCED from upstream
│       ├── aimd.rs               # Our custom AIMD logic
│       └── token_manager.rs      # Our adapted version
│
└── scripts/sync-upstream.sh      # ← SYNC TOOL
```

### Sync Process

```bash
# 1. Fetch upstream changes
git fetch upstream

# 2. Review what changed
git log upstream/main --oneline -10

# 3. Sync proxy code to our crates
./scripts/sync-upstream.sh

# 4. Fix any import path issues
cargo check -p antigravity-core

# 5. Commit
git add -A && git commit -m "feat(sync): upstream v3.3.XX"
```

### Key Fixes Ported (v3.3.32)

- **FIX #632**: Claude tool_result duplicate prevention (pre-scan IDs)
- **FIX #564**: Thinking blocks ordering (thinking first rule)
- **FIX #593**: Deep cache_control cleanup (recursive JSON cleaning)
- **FIX #546/547**: Gemini parameter hallucinations remapping
- **FIX #295**: Function call signature validation

### What Stays Isolated (Never Synced)

- `crates/antigravity-core/src/proxy/aimd.rs` - Our AIMD rate limiting
- `crates/antigravity-core/src/proxy/server.rs` - Our Axum server structure  
- `antigravity-server/` - Our headless daemon
- `src-leptos/` - Our Leptos UI

### Benefits

✅ `git fetch upstream` is safe — no conflicts in src-tauri/
✅ Cherry-pick is trivial: just run sync script
✅ Our custom logic is protected in dedicated files
✅ Clear separation: upstream reference vs our production code