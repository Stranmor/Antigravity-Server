# Antigravity Manager - Architecture Status

## 🏛️ ARCHITECTURAL EVOLUTION [2026-01-17]

**Current Status:** PHASE 4 COMPLETE — antigravity-shared eliminated, direct imports from antigravity-types

### ✅ Completed Phases (1-4)

- **Phase 1:** `antigravity-types` crate, Typed Errors, Protocol types, Resilience API, Prometheus Metrics
- **Phase 2:** Replace symlinks with local copies, Remove `#[path]` includes
- **Phase 3:** Validator integration, Re-exports cleanup, Clippy compliance (all 23 modules clean)
- **Phase 4:** Eliminate `antigravity-shared`, Edition 2021 alignment

### 🔄 Phase 5: Module Size Compliance [IN PROGRESS - 2026-02-02]

**Goal:** Split all files exceeding 300 lines to comply with Single Responsibility Module principle.

| File | Lines | Target Split | Status |
|------|-------|--------------|--------|
| `handlers/openai.rs` | 1728 | `chat.rs`, `completions.rs`, `images.rs`, `models.rs` | ✅ Split into directory |
| `handlers/openai/chat.rs` | 518 | Need further split | 🔄 Phase 5B |
| `handlers/openai/completions.rs` | 650 | Need further split | 🔄 Phase 5B |
| `handlers/openai/images.rs` | 538 | Need further split | 🔄 Phase 5B |
| `handlers/claude.rs` | 1465 | Complex, deferred | ⏸️ Deferred |
| `token_manager/mod.rs` | 1665 | Tests extracted | 🔄 -13% |
| `claude/request.rs` | 1894 | Tests extracted | 🔄 -25% |
| `claude/streaming.rs` | 1177 | TBD | ⏳ |
| `openai/streaming.rs` | 1092 | TBD | ⏳ |
| `api/mod.rs` | 778 | ~~`oauth.rs`~~ ✅, `accounts.rs` | 🔄 -324 lines |
| `rate_limit/mod.rs` | 786 | ~~`types.rs`~~ ✅, ~~`parser.rs`~~ ✅ | 🔄 -289 lines |
| `modules/process.rs` | 1069 | Platform-specific, complex | ⏳ |

**Completed (2026-02-02):**
- `handlers/openai.rs` → `handlers/openai/` directory with `chat.rs`, `completions.rs`, `images.rs`, `models.rs`, `mod.rs`
- Banned filenames renamed: `types.rs` → `messages.rs`, `utils.rs` → `formatters.rs`
- MAX_RETRY_ATTEMPTS unified to 64 across all handlers

### 📊 Architecture (Current)

```
crates/
├── antigravity-types/          # 🔵 SINGLE SOURCE OF TRUTH (canonical definitions)
│   └── src/
│       ├── error/              # AccountError, ProxyError, ConfigError, TypedError
│       ├── models/             # Account, AppConfig, ProxyConfig, QuotaData, TokenData...
│       │   ├── account.rs      # (pub mod)
│       │   ├── config.rs       # (pub mod)
│       │   ├── quota.rs        # (pub mod)
│       │   ├── stats.rs        # (pub mod)
│       │   ├── sync.rs         # (pub mod)
│       │   └── token.rs        # (pub mod)
│       └── protocol/           # OpenAI/Claude/Gemini message types
├── antigravity-client/         # 🟣 RUST SDK (auto-discovery, retry, streaming)
│   └── src/
│       ├── client.rs           # AntigravityClient with auto_discover()
│       ├── error.rs            # ClientError enum
│       └── messages.rs         # ChatRequest, ChatResponse, StreamChunk (SDK-specific)
├── antigravity-core/           # 🟢 BUSINESS LOGIC (all clippy-clean!)
│   └── src/proxy/
│       └── 23 modules          # ALL modules now clippy-clean
├── antigravity-server/         # 🔴 HTTP ENTRY POINT
vendor/
└── antigravity-upstream/       # Git submodule (REFERENCE ONLY)
```

> **Note:** `antigravity-shared` has been ELIMINATED (2026-01-28). All code now imports directly from `antigravity-types`.

### 🎯 Key Metrics

| Metric | Before | After |
|--------|--------|-------|
| Symlinks | 14 | **0** |
| Duplicate type definitions | ~20 | **0** |
| `#[allow(warnings)]` | 11 modules | **0** |
| Clippy warnings suppressed | ~58 | **0** |
| Unit tests | - | **197** |
| Clippy status | ⚠️ | **✅ -D warnings** |
| Release build | - | **11MB** |

### ⏭️ Remaining Tasks

- [x] **Phase 4:** VPS deployment ✅ [2026-01-19] — `https://antigravity.quantumind.ru`
- [ ] **Phase 5:** Module Size Compliance [IN PROGRESS] — see table above
- [x] **Phase 6:** CLI Management — full headless control without Web UI ✅ [2026-01-19]
- [x] **Phase 7:** Rust SDK (`antigravity-client`) — auto-discovery, retry, streaming ✅ [2026-01-19]
- [x] **Phase 7b:** Account auto-sync (60s interval) ✅ [2026-01-19]
- [ ] **Phase 8:** Extract `antigravity-proxy` crate (optional cleanup)

---

## 🧠 SMART ROUTING ARCHITECTURE [2026-01-30]

**Replaces:** Old 3-mode system (CacheFirst/Balance/PerformanceFirst)

### Problem Solved

Thundering herd + cache destruction pattern:
```
10 concurrent requests → Account A
   ↓
Account A: 429 (rate limit)
   ↓
ALL 10 requests switch to Account B
   ↓
Account B: instant 429 (thundering herd)
   ↓
Cache on A — lost, Cache on B — never built
   ↓
cache_hit ≈ 0%
```

### Solution: Unified Smart Routing

| Component | Description |
|-----------|-------------|
| **Least-Connections Selection** | Route to account with fewest active requests (not round-robin) |
| **Per-Account Concurrency Limit** | Max N concurrent requests per account (default: 3) |
| **Isolated Session Migration** | On 429: migrate THIS request only, keep session binding intact |
| **AIMD Pre-emptive Check** | Skip accounts with usage_ratio > 1.2 |

### Configuration (`SmartRoutingConfig`)

```rust
pub struct SmartRoutingConfig {
    pub max_concurrent_per_account: u32,  // default: 3
    pub preemptive_throttle_ratio: f32,   // default: 0.8
    pub throttle_delay_ms: u64,           // default: 100
    pub enable_session_affinity: bool,    // default: true
}
```

### Key Behavioral Changes

| Before | After |
|--------|-------|
| 429 → unbind session → ALL requests migrate | 429 → keep binding → only THIS request migrates |
| QUOTA_EXHAUSTED: 5min lockout | QUOTA_EXHAUSTED: 10min fallback + dynamic protected_models |
| Session stuck on exhausted account | Unbind after 3 consecutive failures OR lockout > 5min |
| Round-robin account selection | Least-connections (min active requests) |
| 3 manual modes to choose | 1 unified algorithm with tunable params |
| No concurrency limit | Max N per account prevents thundering herd |

### Tier-Priority Account Selection [2026-02-01]

**Selection Order:**
| Priority | Tier | Example |
|----------|------|---------|
| 0 | ultra-business | `ws-ai-ultra-business-tier` |
| 1 | ultra | `g1-ultra-tier` |
| 2 | pro | `g1-pro-tier` |
| 3 | free | `free-tier` |
| 4 | unknown | (no tier info) |

**Algorithm:**
1. Filter: exclude rate-limited, quota-protected, already-attempted accounts
2. Sort by: tier_priority (ascending), then active_requests (ascending)
3. Select first available with atomic slot reservation (`try_new`)

**Behavior:**
- All accounts (including ultra) respect rate limits with short lockout (5s)
- Ultra accounts get priority but share load via least-connections within tier
- No bypass of rate-limit checks — prevents hammering single account with 429s

---

## 🛡️ FINGERPRINT PROTECTION [2026-02-01]

### Device Fingerprint API

**Status:** IMPLEMENTED

REST API for managing Cursor/VSCode device fingerprints (storage.json):

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/device/profile` | GET | Read current device profile from storage.json |
| `/api/device/profile` | POST | Generate new profile and write to storage.json |
| `/api/device/backup` | POST | Create timestamped backup of storage.json |
| `/api/device/baseline` | GET | Get original baseline profile |

### Known Limitations

| Protection | Status | Notes |
|------------|--------|-------|
| Device fingerprint API | ✅ IMPLEMENTED | REST endpoints for profile management |
| User-Agent rotation | ❌ REVERTED | Caused CONSUMER_INVALID errors from Google |
| WARP IP isolation | ❌ DISABLED | Google detects WARP → stricter rate limits |
| TLS/JA3 fingerprint | ❌ MISSING | Would require custom TLS config |
| HTTP header randomization | ❌ MISSING | Accept-Language, etc. |

---

## 🔧 API Endpoints

```bash
# Health status (account availability)
GET /api/resilience/health

# Circuit breaker states
GET /api/resilience/circuits

# AIMD rate limiting stats
GET /api/resilience/aimd

# Prometheus metrics
GET /api/metrics
```

---

## ✅ Verification Commands

```bash
cargo check --workspace                        # ✅ passes
cargo clippy --workspace -- -Dwarnings         # ✅ passes
cargo test -p antigravity-types                # ✅ 7 tests pass
cargo test -p antigravity-core --lib           # ✅ 170 tests pass
cargo build --release -p antigravity-server    # ✅ builds (1m 22s, 11MB)
```

---

### Last Sync: 2026-01-29 (v4.0.7)

See git history for detailed sync changelog.

---

## 🔀 UPSTREAM SYNC ARCHITECTURE [2026-01-18]

### Fork Strategy

This fork uses **SEMANTIC PORTING** — we don't blindly copy upstream files, we selectively integrate useful changes while maintaining our own improvements.

### Upstream Reference

- **Location:** `vendor/antigravity-upstream/` (git submodule)
- **Upstream repo:** https://github.com/lbjlaq/Antigravity-Manager
- **Current upstream:** v4.0.4
- **Our version:** v3.3.45 (with custom improvements)

### Intentional Divergences

| File | Lines Diff | Reason |
|------|------------|--------|
| `handlers/claude.rs` | ~1500 | **OUR ADDITIONS:** AIMD rate limiting, resilience patterns, Axum-specific handlers, circuit breakers |
| `handlers/gemini.rs` | ~330 | **COMPLETE REWRITE:** Full Gemini Native API handler with streaming SSE, retry logic, buffer overflow protection |
| `mappers/claude/*.rs` | ~200 | Format differences + our clippy fixes (Rust 1.92 compliance) |
| `mappers/openai/request.rs` | ~100 | **OUR ADDITION:** `tool_result_compressor` for OpenAI endpoint (upstream only has it for Claude) |
| `common/json_schema.rs` | ~20 | Clippy fixes (collapsible_match, etc.) |

### What We Port From Upstream

✅ **ALWAYS PORT:**
- Bug fixes in protocol transformation logic
- New model support (thinking models, signatures)
- JSON Schema improvements (flatten_refs, merge_all_of)
- Security fixes (auth headers, validation)

❌ **NEVER PORT:**
- UI/React code (we use Leptos)
- Tauri-specific code (we use headless Axum)
- Changes that conflict with our resilience layer

### Sync Workflow

```bash
# 1. Update submodule
cd vendor/antigravity-upstream
git fetch origin && git checkout origin/main
cd ../..

# 2. Check what changed in proxy/
git diff HEAD@{1}..HEAD -- vendor/antigravity-upstream/src-tauri/src/proxy/

# 3. Manually port useful changes to our crates/antigravity-core/src/proxy/
# 4. Run clippy + tests
cargo clippy --workspace -- -D warnings
cargo test -p antigravity-core --lib

# 5. Commit
git add . && git commit -m "chore: sync upstream v3.3.XX changes"
```

> **Note:** Detailed sync history moved to git commits. See `git log --grep="sync upstream"` for changelog.

**Our Key Additions (not in upstream):**
- Constant-time API key comparison (timing attack prevention)
- AIMD predictive rate limiting
- Circuit breakers per account
- Prometheus metrics endpoint
- Resilience API endpoints
- WARP proxy support for per-account IP isolation
- Sticky session rebind on 429
  - Intelligent opus fallback → `gemini-3-pro-preview`
  - Multi-wildcard matching (`a*b*c` patterns)

**OUR BUG FIXES (not in upstream):**
- **Race condition in rate limit recording** — fixed immediate lockout in `mark_rate_limited_async()`
- **60s Global Lock missing rate limit check** — added `is_rate_limited()` check
- **protected_models not populated** — replaced `save_account()` with `update_account_quota()`

- **[FEATURE] Smart Warmup Scheduler enabled** (2026-01-26)
  - **Purpose:** Automatically warms up accounts to prevent staleness and maintain active sessions.
  - **Config location:** `~/.antigravity_tools/gui_config.json` → `smart_warmup` section
  - **Config example:**
    ```json
    "smart_warmup": {
      "enabled": true,
      "models": ["gemini-3-flash", "claude-sonnet-4-5", "gemini-3-pro-high", "gemini-3-pro-image", "claude-opus-4-5-thinking"],
      "interval_minutes": 60,
      "only_low_quota": false
    }
    ```
  - **Behavior:**
    - Checks config every 60 seconds, triggers warmup every `interval_minutes` (default 60)
    - **`only_low_quota: false` (default):** Warms up models at 100% quota to prevent staleness
    - **`only_low_quota: true`:** Warms up models below 50% quota to refresh them
    - 4-hour cooldown per model to prevent re-warming
    - Persistent history in `~/.antigravity_tools/warmup_history.json`
  - **Note:** Different from `scheduled_warmup` (old format). Use `smart_warmup` for the scheduler.

- **[FEATURE] Auto Quota Refresh Scheduler** (2026-01-28)
  - **Purpose:** Automatically refreshes account quotas at configurable intervals (mirrors upstream Tauri behavior).
  - **Config location:** `~/.antigravity_tools/gui_config.json` → `auto_refresh` and `refresh_interval` fields
  - **Config example:**
    ```json
    {
      "auto_refresh": true,
      "refresh_interval": 15
    }
    ```
  - **Behavior:**
    - Checks config every 60 seconds
    - If `auto_refresh: true`, refreshes all enabled accounts at `refresh_interval` minute intervals
    - Minimum interval enforced: 5 minutes (values <5 are clamped to 15)
    - After refresh, reloads accounts into AppState for immediate availability
  - **Implementation:** `scheduler::start_quota_refresh()` in `antigravity-server/src/scheduler.rs`
  - **Note:** This was missing in headless server while upstream Tauri had it via `BackgroundTaskRunner.tsx`

---

## ⚠️ KNOWN ARCHITECTURAL QUIRK: Shared Project Rate Limits [2026-01-18]

Rate limits are tracked per **account_id**, but Google Cloud quotas are enforced per **project_id**. If two accounts share the same project, switching between them won't help — both will hit 429.

**Why We DON'T Fix This (Yet):** Google's prompt caching is tied to `project_id`. Switching to another account in the same project might still benefit from cached prompts.

```bash
# Check for shared projects:
cat ~/.antigravity_tools/accounts/*.json | jq -r '.token.project_id' | sort | uniq -c
```

---

## 🔍 BACKEND DISCOVERY: Model Routing [2026-01-18]

| Model Alias | Actual Backend | Evidence |
|-------------|----------------|----------|
| `gpt-4o`, `gpt-4o-mini`, `gpt-*` | **Gemini** (alias) | Responds: "I am gemini-1.5-flash-pro" |
| `gemini-3-pro`, `gemini-*` | **Gemini** (native) | Responds with Antigravity system prompt |
| `claude-opus-4-5`, `claude-*` | **Claude via Vertex AI** | Error contains `req_vrtx_*` request ID |

**Key Insights:** GPT models are fake (Gemini with OpenAI format). Claude models are REAL (Vertex AI partnership).

---

## ⚠️ UNDOCUMENTED OUTPUT TOKEN LIMIT [2026-01-19]

### The Problem

Google Antigravity API has an **undocumented output limit of ~4K tokens** (~150-200 lines of code).

**Symptoms:**
- Stream cuts mid-response without `finish_reason: "max_tokens"`
- Tool call JSON left incomplete/invalid
- Client receives garbage, cannot parse response
- No error message — just silent truncation

**Empirical evidence:** Max observed output in 24h of logs = 3901 tokens.

### What This Means

| Operation | Risk |
|-----------|------|
| Edit tool (small diffs) | ✅ Safe |
| Write tool (<100 lines) | ✅ Safe |
| Write tool (>150 lines) | ❌ Will be truncated |
| README generation | ❌ High risk |
| Full file creation | ❌ High risk |

### Workaround

For large files, use incremental approach:
1. Write skeleton with TODO markers
2. Fill each section with separate Edit calls
3. Each operation <100 lines

**Status:** No fix implemented. Using system prompt workaround (see global AGENTS.md rule 20).

---

## 🚀 ZERO-DOWNTIME DEPLOYMENT [2026-01-19]

### Architecture

Server uses **SO_REUSEPORT** + **Graceful Shutdown** for zero-downtime binary replacement:

```
[OLD process] ← handles requests
      ↓ (deploy trigger)
[OLD] + [NEW] ← BOTH listen on port 8046 via SO_REUSEPORT
      ↓ (SIGTERM → OLD)
[OLD draining] + [NEW accepts new connections]
      ↓ (OLD finishes active requests, exits)
[NEW] ← sole owner of port
```

### Key Components

1. **SO_REUSEPORT** (`socket2` crate) — allows two processes to bind same port
2. **Graceful shutdown** — SIGTERM triggers 30s drain timeout for active connections
3. **systemd service** — `TimeoutStopSec=35` gives time for drain

### Deployment Workflow

Or use: `./scripts/zero-downtime-deploy.sh`

### Container Deployment (Recommended) [2026-01-28]

For production VPS, use containerized deployment via Podman:

```bash
# From project root:
./deploy/deploy-vps.sh
```

**Files:**
- `Containerfile` — Multi-stage build (Rust + Trunk frontend)
- `deploy/antigravity.container` — Quadlet systemd unit
- `deploy/deploy-vps.sh` — Automated deployment script

**Note:** First build takes ~15min (Rust compilation). Subsequent builds are faster due to layer caching.

### Important: Unified Build

**Backend and frontend are built together** via `build.rs`:

This means `cargo build -p antigravity-server` builds BOTH:
- Rust backend binary
- Leptos WASM frontend (via trunk)

**DO NOT deploy backend without rebuilding frontend** — they share the same release cycle.

---

## 📦 BUILD SYSTEM [2026-01-19]

### Build Commands

| Command | What it builds |
|---------|---------------|
| `cargo build -p antigravity-server` | Backend + Frontend (via build.rs) |
| `trunk build` (in src-leptos/) | Frontend only |
| `cargo build -p antigravity-leptos` | Frontend crate only (no WASM) |

### Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `ANTIGRAVITY_STATIC_DIR` | `./src-leptos/dist` | Path to frontend assets |
| `ANTIGRAVITY_PORT` | `8045` | Server port |
| `SKIP_TRUNK_BUILD` | unset | Skip frontend build in CI |
| `ANTIGRAVITY_SYNC_REMOTE` | unset | Remote server URL for bidirectional config sync |

---

## 🔄 MODEL ROUTING SYNC [2026-01-27]

### Overview

Bidirectional synchronization of `custom_mapping` (model routing) between VPS and local instances using **Last-Write-Wins (LWW)** merge strategy — the same approach used by AWS DynamoDB and Apache Cassandra for distributed state.

### How It Works

```
VPS (antigravity.quantumind.ru)          Local Machine
┌─────────────────────────────┐         ┌─────────────────────────┐
│  SyncableMapping (MASTER)   │ ◄─────► │  SyncableMapping        │
│                             │         │                         │
│  GET /api/config/mapping    │         │  Auto-sync every 60s    │
│  POST /api/config/mapping   │         │  (if ANTIGRAVITY_SYNC_  │
└─────────────────────────────┘         │   REMOTE is set)        │
                                        └─────────────────────────┘
```

### LWW Merge Strategy

Each mapping entry has a timestamp. On merge:
- If only in local → keep local
- If only in remote → add from remote  
- If in both → keep the one with **higher timestamp**
- On timestamp tie → lexicographic comparison of `target` as tie-breaker

No conflicts. Eventual consistency guaranteed.

### Tombstone Support

Deletions use **tombstones** (soft delete) to prevent "zombie resurrection" during sync:
- `remove(key)` → inserts tombstone entry with `deleted: true`
- Tombstones propagate via LWW like regular entries
- `get()`, `len()`, `to_simple_map()` exclude tombstones
- `total_entries()` includes tombstones (for debugging)

### API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/config/mapping` | GET | Get current mappings with timestamps |
| `/api/config/mapping` | POST | Merge remote mappings (LWW) |

### Usage

**Enable sync on local machine:**
```bash
export ANTIGRAVITY_SYNC_REMOTE="https://antigravity.quantumind.ru"
antigravity-server
```

**Manual sync via API:**
```bash
# Get current mappings from VPS
curl https://antigravity.quantumind.ru/api/config/mapping

# Push local changes to VPS
curl -X POST https://antigravity.quantumind.ru/api/config/mapping \
  -H "Content-Type: application/json" \
  -d '{"mapping": {"entries": {"gpt-4o": {"target": "gemini-3-pro", "updated_at": 1737932400000}}}}'
```

### Sync Flow

1. Every 60s, local fetches remote mappings
2. LWW merge: newer entries overwrite older
3. Local sends back any entries that are newer locally
4. Both instances converge to identical state

### Known Limitations

- **Tombstone persistence:** Currently tombstones are not fully persisted. Deletions set `target=""` which effectively disables the mapping, but the key remains in storage. Full tombstone garbage collection is TODO.
- **Storage format:** Runtime uses `SyncableMapping` with timestamps, but persistence uses legacy `HashMap<String, String>` format in `gui_config.json`. Timestamps are stored separately in memory.

### Files

- `crates/antigravity-types/src/models/sync.rs` — `SyncableMapping`, `MappingEntry`, LWW merge logic
- `antigravity-server/src/config_sync.rs` — Auto-sync background task
- `antigravity-server/src/state.rs` — `get_syncable_mapping()`, `merge_remote_mapping()`
- `antigravity-server/src/api.rs` — `/api/config/mapping` endpoints

