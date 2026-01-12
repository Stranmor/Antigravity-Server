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
- [ ] Create `antigravity-server` binary (Axum + static files + proxy logic)
- [ ] Move proxy handlers from `src-tauri` to `antigravity-server`
- [ ] Add REST API endpoints matching Tauri IPC commands
- [ ] Serve `src-leptos/dist/` as static files
- [ ] Update systemd service to run `antigravity-server`
- [ ] Create `ag` CLI that calls HTTP API
- [ ] Delete `src-tauri/` after verification

---

## Current Status: Architecture Migration In Progress

**UI Status:** Leptos UI complete (100% parity with React)
**Backend Status:** Core logic in `antigravity-core`, handlers in `src-tauri`
**Build Status:** Tauri build works but UI doesn't render (WebKit issue)

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