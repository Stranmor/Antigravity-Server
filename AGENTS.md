# Antigravity Manager - Architecture Status

## ✅ COMPLETED: PostgreSQL Migration [2026-02-03]

**Goal:** Replace JSON file storage with PostgreSQL + Event Sourcing — **DEPLOYED**

### Verification Results

| Metric | Value |
|--------|-------|
| Accounts migrated | 41 |
| Tokens migrated | 41 |
| API `/api/accounts` | ✅ Working |
| API `/api/status` | ✅ Working |
| Chat completions | ✅ Working |
| Database | PostgreSQL 16 on VPS |

### Migration Phases

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Add sqlx + PostgreSQL deps | ✅ |
| 2 | Create migration files (schema) | ✅ |
| 3 | Implement `AccountRepository` trait | ✅ |
| 4 | PostgreSQL backend implementation | ✅ |
| 5 | Event sourcing: `AccountEvent` enum | ✅ |
| 6 | JSON → PostgreSQL data migration | ✅ |
| 7 | Wire repository into AppState + main.rs | ✅ |
| 8 | Setup PostgreSQL on VPS (NixOS) | ✅ |
| 9 | Update API handlers to use repository | ✅ |
| 10 | Deploy + verify | ✅ |

### Database Configuration

| Setting | Value |
|---------|-------|
| Host | 127.0.0.1 |
| Database | antigravity |
| User | antigravity |
| Tables | accounts, tokens, quotas, account_events, requests, app_settings |

### Files Modified

- `Cargo.toml` (workspace) — added sqlx, async-trait
- `crates/antigravity-core/Cargo.toml` — added sqlx, async-trait
- `crates/antigravity-core/migrations/001_initial_schema.sql` — PostgreSQL schema (with IF NOT EXISTS)
- `crates/antigravity-core/src/modules/repository.rs` — AccountRepository trait
- `crates/antigravity-core/src/modules/account_pg.rs` — PostgreSQL implementation
- `crates/antigravity-core/src/modules/json_migration.rs` — Migration utilities
- `antigravity-server/Cargo.toml` — added sqlx
- `antigravity-server/src/main.rs` — DATABASE_URL parsing, PostgresAccountRepository init
- `antigravity-server/src/state.rs` — Added repository to AppState
- `antigravity-server/src/api/mod.rs` — Updated handlers to use repository when available
- NixOS config `/etc/nixos/configuration.nix` — Added DATABASE_URL to antigravity.service

---

## 🏛️ ARCHITECTURAL EVOLUTION [2026-02-02]

**Current Status:** PHASE 5 IN PROGRESS — Module size compliance refactoring

### ✅ Completed Phases (1-4)

- **Phase 1:** `antigravity-types` crate, Typed Errors, Protocol types, Resilience API, Prometheus Metrics
- **Phase 2:** Replace symlinks with local copies, Remove `#[path]` includes
- **Phase 3:** Validator integration, Re-exports cleanup, Clippy compliance (all 23 modules clean)
- **Phase 4:** Eliminate `antigravity-shared`, Edition 2021 alignment

### 🔄 Phase 5: Module Size Compliance [COMPLETE - 2026-02-03]

**Goal:** Split all files exceeding 300 lines to comply with Single Responsibility Module principle.

**Status:** ✅ ALL core `.rs` files now under 300 lines.

**Remaining (exempt or low priority):**
- Test files (exempt): `request_tests.rs` (614), `handlers.rs` (378)
- Leptos UI (low priority): `settings.rs` (549), `dashboard.rs` (399), `add_account_modal.rs` (379)

**Completed refactoring:**
- `mappers/claude/request.rs` → `mappers/claude/request/` directory (13 modules) ✅
- `handlers/claude.rs` → `handlers/claude/` directory (5 modules) ✅
- `handlers/openai.rs` → `handlers/openai/` directory ✅
- `token_manager/mod.rs` → 13 modules ✅
- `mappers/claude/streaming.rs` → `mappers/claude/streaming/` directory (7 modules) ✅
- `mappers/openai/streaming.rs` → `mappers/openai/streaming/` directory (6 modules) ✅
- `mappers/gemini/wrapper.rs` → extracted tests to `wrapper_tests.rs` ✅
- `modules/device.rs` → extracted tests to `device_tests.rs` ✅
- `antigravity-server/main.rs` → extracted `server_utils.rs` + `router.rs` ✅

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
│   └── src/
│       ├── modules/            # Account storage, repository, JSON migration
│       └── proxy/
│           └── 23 modules      # ALL modules now clippy-clean
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

- [x] **VPS deployment** ✅ [2026-01-19] — `https://antigravity.quantumind.ru`
- [ ] **Phase 5:** Module Size Compliance [IN PROGRESS] — see table above
- [x] **CLI Management** — full headless control without Web UI ✅ [2026-01-19]
- [x] **Rust SDK** (`antigravity-client`) — auto-discovery, retry, streaming ✅ [2026-01-19]
- [x] **Account auto-sync** (60s interval) ✅ [2026-01-19]
- [ ] **Extract `antigravity-proxy` crate** (optional cleanup)

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

### Tier-Priority Account Selection [2026-02-02]

**Selection Order:**
| Priority | Tier | Example |
|----------|------|---------|
| 0 | ultra-business | `ws-ai-ultra-business-tier` |
| 1 | ultra | `g1-ultra-tier` |
| 2 | pro | `g1-pro-tier` |
| 3 | free | `free-tier` |
| 4 | unknown | (no tier info) |

**Algorithm (updated 2026-02-02):**
1. **Ultra-tier priority:** Check ultra/ultra-business accounts (tier 0-1) FIRST
2. If ultra available → use it (even if session is sticky to pro account)
3. If no ultra available → check sticky session binding (fallback)
4. If no sticky → standard tier-priority selection from remaining accounts
5. Filter at each step: exclude rate-limited, quota-protected, already-attempted
6. Sort by: tier_priority (ascending), then active_requests (ascending)

**Behavior:**
- Ultra accounts OVERRIDE sticky sessions — if ultra is available, it's used
- Sticky session acts as FALLBACK when no ultra accounts are available
- All accounts respect rate limits with short lockout (5s)
- Ultra accounts share load via least-connections within tier

**Example Flow:**
| Scenario | Sticky on Pro | Ultra Available | Result |
|----------|---------------|-----------------|--------|
| Normal | Account A (pro) | Account B (ultra) free | B selected (ultra priority) |
| Ultra busy | Account A (pro) | B rate-limited | A selected (sticky fallback) |
| Both busy | Account A (pro) | B rate-limited, A rate-limited | Next available by tier |

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
```

### Load Testing (MANDATORY for routing/selection changes)

Changes to account selection, routing, or rate limiting logic MUST be verified with load testing before production deployment.

**Test command (50 concurrent requests):**
```bash
for i in $(seq 1 50); do
  curl -s -X POST "https://antigravity.quantumind.ru/v1/chat/completions" \
    -H "Authorization: Bearer $API_KEY" \
    -H "Content-Type: application/json" \
    -d "{\"model\": \"gemini-3-pro\", \"messages\": [{\"role\": \"user\", \"content\": \"Say $i. ID: $(uuidgen)\"}], \"max_tokens\": 50}" &
done
wait
```

**Success criteria:**
- 100% success rate (all HTTP 200)
- No thundering herd (requests distributed across multiple accounts)
- Total time < 30s for 50 requests

**When required:**
- Any change to `token_manager/mod.rs`
- Any change to `rate_limit/` modules
- Any change to account selection/routing logic
cargo test -p antigravity-core --lib           # ✅ 170 tests pass
cargo build --release -p antigravity-server    # ✅ builds (1m 22s, 11MB)
```

---

## 🔀 Upstream Sync

- **repo:** lbjlaq/Antigravity-Manager
- **watch:** src-tauri/src/proxy/, src-tauri/src/modules/
- **ignore:** *.tsx, *.json, README*, i18n/, Tauri-specific
- **last_reviewed:** 9033f4f (2026-02-03)

### What We Port

✅ Bug fixes in protocol transformation, new model support, JSON Schema improvements, security fixes

❌ UI/React, Tauri-specific, changes conflicting with our resilience layer

### Our Divergences

| Area | Description |
|------|-------------|
| Routing | Smart routing with least-connections (not P2C/round-robin) |
| Resilience | AIMD rate limiting, circuit breakers, health scores |
| Handlers | Axum-specific, streaming SSE, buffer overflow protection |
| Security | Constant-time API key comparison |

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

Use: `./scripts/zero-downtime-deploy.sh`

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

