# Antigravity Manager - Architecture Status

## TARGET GOAL
- Fix Claude retry signature handling: narrow signature error detection and avoid tool-loop closure during recovery (completed).
- Fix Claude/Vertex tool_use/tool_result pairing after signature stripping (completed).
- Fix quota persistence and dual-write consistency bugs (completed).
- Fix production crash: blocking TLS init in async runtime (completed).
- Enforce image MIME auto-detection from bytes across proxy mappers (completed).
- Eliminate duplicated retry logic across handlers (completed).
- Introduce ModelFamily enum and replace model family string checks (completed).

## Current Status
- ✅ COMPLETED [2026-02-10]: Fixed Claude retry signature handling. Root cause: `CLAUDE_SIGNATURE_PATTERNS` included bare `"INVALID_ARGUMENT"` which matched ALL 400 errors (including MIME mismatches), triggering wasteful signature recovery + message inflation (5→41 messages). Fix: removed overly broad pattern, removed `close_tool_loop_for_thinking()` from error recovery (already handled during preprocessing). Added 24 tests.
- ✅ COMPLETED [2026-02-10]: Fixed tool_use/tool_result orphaning in Claude via Vertex AI path. Root cause: signature stripping dropped model messages that became empty after thinking removal, breaking role alternation. Fix: empty model messages now get placeholder text part instead of being dropped. Added diagnostic validation for functionCall/functionResponse pairing.
- ✅ COMPLETED [2026-02-10]: [FIX #1740] Fixed tool_use/tool_result pairing breakage with multiple image tool_results on Claude Vertex AI. Root cause: when multiple tool_results contained images, `build_tool_result_part` emitted parts as `[functionResponse(A), inlineData(A), functionResponse(B), inlineData(B), ...]`. Claude Vertex API converts Gemini format back to Claude format, where `inlineData` between `functionResponse` parts breaks the tool_use→tool_result chain — Claude sees `tool_result(A)` ✅ → unrelated `image` → `tool_result(B)` ❌. Single-image tool_results worked fine because there was no interleaving. Fix: (1) Added `functionResponse` as a separate bucket in `reorder_gemini_parts()` that sorts before `other_parts` (inlineData), producing `[funcResp(A), funcResp(B), funcResp(C), inlineData(A), inlineData(B), inlineData(C)]`; (2) Called `reorder_gemini_parts()` in `build_google_content()` for every message, not just during `merge_adjacent_roles` — previously single user messages with multiple tool_results were never reordered. Images are preserved and visible to the model.
- ✅ COMPLETED [2026-02-10]: Fix quota persistence and dual-write consistency bugs.
- ✅ COMPLETED [2026-02-10]: Consolidated retry logic across handlers, using unified retry helpers and constants.
- ✅ COMPLETED [2026-02-10]: Added image MIME detection from base64 magic bytes and wired it across proxy mappers to override declared types when needed.
- ✅ COMPLETED [2026-02-10]: Removed all image size and count limits from tool result processing. Images of any size and quantity now pass through to Gemini without filtering.
- ✅ COMPLETED [2026-02-10]: Introduced ModelFamily enum and replaced model family string checks in the specified call sites.
- ✅ COMPLETED [2026-02-10]: Graceful stream truncation: when Gemini silently truncates output (no finish_reason while inside a content block) or mid-stream network errors occur, the proxy now emits a **normal completion** (`stop_reason: "max_tokens"` for Claude / `finish_reason: "length"` for OpenAI) instead of `overloaded_error`. This prevents AI agents from endlessly retrying requests that will always be truncated. Fallback plan: if agents still retry on `max_tokens`, add `[Response truncated by proxy]` text marker.
- ✅ COMPLETED [2026-02-10]: Added sliding window image retention: strips images from user messages older than 5 turns to prevent context overflow in long agentic sessions. Wired into all 3 request paths (Claude, OpenAI, Gemini native). Also fixed monitor middleware to capture error response bodies and removed diagnostic logging. Deployed locally + VPS.
- ✅ COMPLETED [2026-02-09]: Fixed production crash caused by blocking native-TLS initialization inside tokio async runtime. `UpstreamClient::new()` now accepts pre-built `reqwest::Client`; proxy/WARP client builds wrapped in `spawn_blocking`. Deployed and verified on VPS.
- ✅ COMPLETED [2026-02-09]: Fixed production outage — all requests failing "All accounts exhausted". Root cause: `upstream_proxy` in `gui_config.json` was `enabled: true` pointing to `http://127.0.0.1:8046` where nothing was listening. Every outgoing request got instant `Connection refused`, all 16 accounts cycled through and marked exhausted. Fix: disabled upstream proxy in config. Additionally promoted transport error logging from `debug` to `error` level for future visibility.

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
| Tables | accounts, tokens, quotas, account_events, requests, app_settings, thinking_signatures, session_signatures |

### Database Replication [2026-02-08]

VPS PostgreSQL is replicated to the home-server via streaming replication. See the global AGENTS.md for the full schema.

| Role | Location | Connection string |
|------|-----|-------------------|
| **Primary (read-write)** | VPS | `postgres://antigravity@localhost/antigravity?host=/run/postgresql` |
| **Replica (read-only)** | home-server:5436 | `postgres://antigravity@192.168.0.124:5436/antigravity` |

- VPS: `wal_level=replica`, `max_wal_senders=10`, `wal_keep_size=1GB`
- Replication user: `replicator` (password stored in global AGENTS.md)
- SSH tunnel: `pg-replication-tunnel.service` on home-server

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

**Current Status:** PHASE 5 COMPLETE — Module size compliance refactoring

### ✅ Completed Phases (1-4)

- **Phase 1:** `antigravity-types` crate, Typed Errors, Protocol types, Resilience API, Prometheus Metrics
- **Phase 2:** Replace symlinks with local copies, Remove `#[path]` includes
- **Phase 3:** Validator integration, Re-exports cleanup, Clippy compliance (all 23 modules clean)
- **Phase 4:** Eliminate `antigravity-shared`, Edition 2021 alignment

### 🔄 Phase 5: Module Size Compliance [COMPLETE - 2026-02-04]

**Goal:** Split all files exceeding 300 lines to comply with Single Responsibility Module principle.

**Status:** ✅ ALL `.rs` files now under 300 lines (except test files which are exempt).

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
- `src-leptos/pages/settings.rs` (549) → `settings/` directory (7 modules, max 134 lines) ✅
- `src-leptos/pages/dashboard.rs` (399) → `dashboard/` directory (4 modules, max 217 lines) ✅
- `src-leptos/components/add_account_modal.rs` (379) → `add_account_modal/` directory (3 modules) ✅
- `modules/migration.rs` (306) → extracted `token_extraction.rs` (265 lines) ✅

**Exempt (test files):**
- `request_tests.rs` (614 lines)
- `handlers.rs` (378 lines)

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
| Unit tests | - | **346** |
| Integration tests | - | **1** |
| Clippy status | ⚠️ | **✅ -D warnings** |
| Release build | - | **11MB** |

### ⏭️ Remaining Tasks

- [x] **VPS deployment** ✅ [2026-01-19] — `https://antigravity.quantumind.ru`
- [x] **Phase 5:** Module Size Compliance ✅ [2026-02-04]
- [x] **CLI Management** — full headless control without Web UI ✅ [2026-01-19]
- [x] **Rust SDK** (`antigravity-client`) — auto-discovery, retry, streaming ✅ [2026-01-19]
- [x] **Account auto-sync** (60s interval) ✅ [2026-01-19]
- [ ] **Extract `antigravity-proxy` crate** (optional cleanup)

### ⚠️ Known Issues (Tech Debt)

| File | Issue | Severity |
|------|-------|----------|
| `proxy/common/json_schema/recursive.rs` | `if/else if/else` for `properties`/`items` is mutually exclusive — if schema has BOTH, `items` won't be recursively cleaned | Medium |
| `proxy/common/json_schema/recursive.rs` | `else` fallback block treats ALL remaining fields as schemas — data fields like `enum` values or `const` containing object-like structures could be corrupted by normalization | Low |
| `proxy/common/json_schema/recursive.rs` | Unbounded recursion in schema cleaning can overflow stack on deeply nested input | Medium |
| `modules/repository.rs` | `update_token_credentials` accepts both `expires_in` and `expiry_timestamp` — redundant, allows conflicting data | Low |
| `modules/oauth.rs` | Hardcoded OAuth client secret makes rotation difficult | Low |
| `proxy/mappers/tool_result_compressor/mod.rs` | Regexes are recompiled on each call to HTML cleaning, wasting CPU in hot path | Medium |
| `antigravity-server/src/api/` | Blocking file I/O in async handlers can stall request processing under load | Medium |
| `proxy/middleware/monitor.rs` | ~~DoS Risk: parses entire request/response bodies (up to 100MB) into JSON DOM.~~ **Partially mitigated [2026-02-09]**: Actual limit is 2MB, not 100MB. SSE uses mpsc channel (streaming preserved). Remaining: 2MB buffer per connection under high concurrency still causes memory pressure. | Medium |
| `proxy/middleware/monitor.rs` | ~~Latency: `handle_json_response` buffers full response before forwarding.~~ **Partially mitigated [2026-02-09]**: Actually uses mpsc channel — client receives data as it arrives. Server-side buffering up to 2MB for usage extraction. | Low |
| `proxy/middleware/monitor.rs` | ~~Request body handling returns `Body::empty()` on buffering failure.~~ **Fixed [2026-02-09]**: Actually returns 502 Bad Gateway on request buffering failure (code was misread). | ~~High~~ Fixed |
| `proxy/middleware/monitor.rs` | Inefficient: attempts to parse all `text/*` as JSON. | Low |
| `proxy/mappers/claude/request/tool_result_handler.rs` | ~~Invalid Part schema: mixing `functionResponse` and `inlineData` in interleaved order breaks Claude Vertex tool pairing.~~ **Fixed [2026-02-10]** [FIX #1740]: `reorder_gemini_parts()` now groups `functionResponse` before `inlineData`; called for every message in `build_google_content()`. Remaining: `thoughtSignature` injected alongside `functionResponse` may still violate Gemini Part union expectations. | ~~High~~ Low |
| `proxy/mappers/openai/request/content_parts.rs` | Blocking file reads, missing percent-decoding for `file://` paths, and unbounded memory for large videos when base64-encoding. | High |
| `proxy/providers/zai_anthropic.rs` | DoS Risk: `deep_remove_cache_control` logs at `info` for every field, vulnerable to log flooding. | Medium |
| `proxy/providers/zai_anthropic.rs` | Inefficient: `copy_passthrough_headers` performs unnecessary string allocations in hot path. | Low |
| `modules/proxy_db.rs` | Data Loss: `save_log` hardcodes `request_body`/`response_body` to `None`. | Medium |
| `modules/proxy_db.rs` | Blocking SQLite I/O via `rusqlite` runs on async runtime threads (thread_local connection), risking executor starvation under load. | High |
| `server_utils.rs` | Portability: `set_reuse_port(true)` lacks `#[cfg(unix)]` guard. | Low |
| `repository.rs` | Inconsistent time types (DateTime vs i64) and redundant arguments in `update_token_credentials`. | Low |
| `state/mod.rs` | ~~`build_proxy_router` ignores `UpstreamProxyConfig` from `proxy_config` — passes default instead of actual config.~~ **Fixed [2026-02-09]**: Investigation showed `build_proxy_router` does pass its argument correctly. The real issue was `UpstreamClient` creating a separate `Arc<RwLock<>>` — now shares the same reference as `AppState.upstream_proxy`. | ~~High~~ Fixed |
| `account_pg_targeted.rs` | `project_id` stored in `tokens` instead of `accounts`; `update_token_credentials_impl` overwrites all sessions. | Medium |
| `token_manager/mod.rs` | Arbitrary session eviction (hash order, not LRU); race condition in active request cleanup. | Medium |
| `token_manager/selection.rs` | `get_token_forced` bypasses expiry check and project ID lookup. | Medium |
| `token_manager/selection_helpers.rs` | Sort comparator calls `get_active_requests` per comparison — O(N log N) DashMap lookups. Should pre-fetch. | Medium |
| `token_manager/token_refresh.rs` | `refresh_locks` DashMap entries never removed — memory leak proportional to unique account_ids. | Medium |
| `proxy/server.rs` | ~~`build_proxy_router_with_shared_state` takes `upstream_proxy` by value, breaking hot-reload.~~ **Fixed [2026-02-09]**: `UpstreamClient::new` now accepts `Arc<RwLock<UpstreamProxyConfig>>` directly, sharing the same reference as `AppState`. `auth_middleware` gets `security_config` instead of full `AppState`. | ~~High~~ Fixed (proxy part) |
| `modules/json_migration.rs` | `migrate_json_to_postgres` counts partial migration as success; `verify_migration` fails if PG has more accounts than JSON. | Low |
| `modules/account/fetch.rs` | Race condition: concurrent fetches can cause lost updates (token/quota desync) | High |
| `token_manager/selection_helpers.rs` | Thundering herd: pre-calculated load snapshot can lead to skewed distribution during bursts | Medium |
| `proxy/handlers/` | ~~OOM Risk: 50MB buffer per stream can exhaust VPS RAM under high concurrency~~ **Mitigated [2026-02-09]**: Reduced to 10MB per stream. Still a concern at extreme concurrency (1000+ streams = 10GB). | ~~High~~ Medium |
| `state/accessors.rs` | Race condition: `hot_reload_proxy_config` updates fields non-atomically. | High |
| `state/accessors.rs` | Logic error: `get_model_quota` uses `.contains()` instead of `.starts_with()`. | Medium |
| `state/accessors.rs` | Split-brain: `switch_account` updates file before repository. | Medium |
| `state/mod.rs` | Potential bug: `proxy_config.experimental` may cause partial-move if `ExperimentalConfig` stops deriving `Copy`. Currently works because `Copy` is derived. | Low |
| `state/mod.rs` | Bug: `health_monitor` shared instance vs separate instances in `token_manager`. | Medium |
| `state/accessors.rs` | `get_account_count` swallows DB errors and returns 0 silently. | Medium |
| `state/accessors.rs` | `load_config()` performs sync file I/O inside async context. | Medium |
| `state/accessors.rs` | `generate_oauth_state` clears ALL pending states when limit hit (aggressive eviction). | Low |
| `proxy/server.rs` | `UpstreamClient` initialized with `None` for circuit_breaker — circuit breaking disabled for upstream. | Medium |
| `proxy/server.rs` | `provider_rr` and `zai_vision_mcp_state` recreated on each `build_proxy_router_with_shared_state` call — state lost on hot-reload. | Medium |
| `proxy/server.rs` | `/v1/api/event_logging` and `/v1/api/event_logging/batch` are stubs returning 200 OK without processing data. | Low |
| `upstream/client/mod.rs` | `get_client()` silently falls back to direct connection on proxy build failure — traffic leak risk when proxy is intentionally configured. | Medium |
| `upstream/client/mod.rs` | Single-slot client cache (`proxied_client`, `warp_client`) thrashes if alternating proxy URLs. | Low |
| `antigravity-vps-cli/src/main.rs` | CLI argument parsing flattens arguments with `join(" ")`, breaking quoted/space-containing args. | Medium |
| `deploy.sh` / `flake.nix` | **Nix closure deploy causes SIGBUS on VPS** [2026-02-10]: Binary built via `nix build .#antigravity-server` crashes immediately with SIGBUS (signal 7) on VPS despite both machines being x86_64 AMD Zen4. `ldd` also crashes on the binary (exit 135). Root cause unclear — possibly Nix closure linking incompatibility. **Workaround:** Deploy via `scp target/release/antigravity-server` (cargo-built binary works fine). `./deploy.sh deploy` is currently BROKEN for VPS. | High |
| `proxy/handlers/claude/streaming.rs` | Unreachable `None` branch in `handle_streaming_response` match — `poll_next` on non-empty stream should always return `Some`. Dead code. | Low |
| `proxy/handlers/claude/chat.rs` | Double boxing in `collect_to_json_response`: `Box<dyn Error>` error wrapped again by `?` into another `Box<dyn Error>`. Unnecessary allocation. | Low |
| `proxy/mappers/openai/streaming/openai_stream.rs` | Shared mutable `tool_call_state` across candidates — if `n > 1` (multiple completions), tool call accumulation would cross-contaminate. Currently safe because Gemini only returns `n=1`. | Low |
| `proxy/handlers/claude/chat.rs` | Error masking in non-streaming (collect_to_json) path: on stream error, returns last buffered JSON or generic error — original error details lost. Acceptable tradeoff vs infinite retries. | Low |

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
| WARP IP isolation | ❌ REMOVED | Module deleted [2026-02-08] — Google detects WARP → stricter rate limits |
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

```bash
cargo test -p antigravity-core --lib           # ✅ 170 tests pass
cargo build --release -p antigravity-server    # ✅ builds (1m 22s, 11MB)
```

---

## 🔀 Upstream Sync

- **repo:** lbjlaq/Antigravity-Manager
- **watch:** src-tauri/src/proxy/, src-tauri/src/modules/
- **ignore:** *.tsx, *.json, README*, i18n/, Tauri-specific
- **last_reviewed:** f86a58d (2026-02-08)

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

**Status:** No fix implemented. Using system prompt workaround (see global AGENTS.md).

---

## ⚠️ MANDATORY thoughtSignature ON functionCall PARTS [2026-02-08]

Google now **requires** `thoughtSignature` field on ALL `functionCall` parts in request body. Without it → 400 INVALID_ARGUMENT.

**Error message:** `"Function call is missing a thought_signature in functionCall parts. This is required for tools to work correctly."`

**Docs:** https://ai.google.dev/gemini-api/docs/thought-signatures

**Fix applied:** Both OpenAI path (`message_transform.rs`) and Gemini native path (`wrapper.rs`) now inject `"skip_thought_signature_validator"` dummy signature when no real signature is cached. This dummy is documented by Google as a valid bypass.

**Injection points:**
| Path | File | Behavior |
|------|------|----------|
| OpenAI | `mappers/openai/request/message_transform.rs:154` | Session cache → fallback to dummy |
| Gemini native | `mappers/gemini/wrapper.rs:23` | Session cache → fallback to dummy |
| Claude/Anthropic | `mappers/claude/request/content_builder.rs:253` | Already had dummy fallback (unaffected) |

**Scope:** Applies to ALL Gemini models. Claude via Vertex AI is unaffected (different protocol).

---

## ⚠️ INPUT TOKEN LIMIT — CLAUDE VIA VERTEX AI [2026-02-07]

Vertex AI enforces a **hard 200,000 token limit** on Claude prompt input.

| Test | Prompt Tokens | Result | Time |
|------|---------------|--------|------|
| ~200K chars | 170,841 | ✅ 200 OK | 6.4s |
| ~1.2M chars | 278,399 | ❌ 400 `prompt is too long: 278399 tokens > 200000 maximum` | 1.6s |

**Error format:** `{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: N tokens > 200000 maximum"}}` with `request_id: req_vrtx_*`

**Source of limit:** Google Vertex AI side (confirmed by `req_vrtx_*` request ID), not Antigravity proxy.

**Note:** Gemini models have separate limits (1M+ context window). This 200K limit applies ONLY to Claude models routed through Vertex AI.

---

## 🚀 DEPLOYMENT [2026-02-07]

### Canonical Path: Nix Closure

**Single entry point:** `./deploy.sh <command>`

| Command | Description |
|---------|-------------|
| `./deploy.sh deploy` | Build Nix closure → copy to VPS → restart service |
| `./deploy.sh rollback` | Restore previous version from `.previous` backup |
| `./deploy.sh status` | Show service status, health, current/previous binary |
| `./deploy.sh deploy-local` | Zero-downtime local deploy (socket activation) |
| `./deploy.sh logs [-n N]` | Stream VPS service logs |

**Flow:**
```
Local: nix build .#antigravity-server
  → nix copy --to ssh://vps-production (closure with all deps)
  → rsync frontend dist/
  → SSH: symlink binary, write systemd unit, restart
  → Health check: /api/health
```

**Rollback:** Each deploy saves previous binary path to `/opt/antigravity/.previous`. Rollback restores symlink + restarts.

### Zero-Downtime (Local) — Socket Activation [2026-02-08]

Local service uses **systemd socket activation** for zero-downtime deploys:

```
antigravity-manager.socket ← owns port 8045 (always listening)
         ↓ (first connection)
antigravity-manager.service ← started by systemd, receives fd=3
         ↓ (deploy: systemctl restart)
[OLD draining] ← socket stays open, kernel buffers new connections (backlog=4096)
         ↓ (OLD exits, NEW starts, receives fd=3)
[NEW] ← processes buffered connections (~3s latency during restart)
```

**Command:** `./deploy.sh deploy-local` (build → replace binary → restart service)

**Units:**
- `~/.config/systemd/user/antigravity-manager.socket` — ListenStream=127.0.0.1:8045
- `~/.config/systemd/user/antigravity-manager.service` — Requires=antigravity-manager.socket

### Important: Unified Build

**Backend and frontend are built together** via `build.rs`. `cargo build -p antigravity-server` builds BOTH Rust backend and Leptos WASM frontend (via trunk).

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
- `antigravity-server/src/api/mod.rs` — `/api/config/mapping` endpoints
