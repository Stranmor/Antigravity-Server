# Antigravity Manager - Architecture Status

## 🏛️ ARCHITECTURAL EVOLUTION [2026-01-17]

**Current Status:** PHASE 3c COMPLETE — Full Clippy Compliance

### ✅ Completed Phases

| Phase | Task | Status |
|-------|------|--------|
| **1** | Created `antigravity-types` crate (foundation types, error hierarchy) | ✅ |
| **1** | Typed Errors (`AccountError`, `ProxyError`, `ConfigError` + `TypedError`) | ✅ |
| **1** | Protocol types (`OpenAI`, `Claude`, `Gemini` message types) | ✅ |
| **1** | Unit tests for types crate (7 tests passing) | ✅ |
| **1** | Clippy Compliance — workspace passes `-D warnings` | ✅ |
| **1** | Resilience API (`/api/resilience/*`) | ✅ |
| **1** | Prometheus Metrics (`/api/metrics`) | ✅ |
| **2** | Replace symlinks with local copies | ✅ |
| **2** | Remove `#[path]` includes from common/ | ✅ |
| **3a** | Add `validator::Validate` to all config types in `antigravity-types` | ✅ |
| **3a** | Replace `antigravity-shared/src/models/*` with re-exports | ✅ |
| **3a** | Replace `antigravity-shared/src/error.rs` with re-exports | ✅ |
| **3a** | Replace `antigravity-shared/src/proxy/config.rs` with re-exports | ✅ |
| **3a** | Update `antigravity-core/src/lib.rs` docstring | ✅ |
| **3b** | Clean `sticky_config.rs` → re-export layer | ✅ |
| **3b** | Add `warp_isolation.rs` module | ✅ |
| **3b** | Reorganize `proxy/mod.rs` into STRICT/CLEANUP sections | ✅ |
| **3b** | Fix flaky test in `error_classifier.rs` | ✅ |
| **3c** | Remove `#[allow(warnings)]` from all 11 modules | ✅ |
| **3c** | Fix ~58 Rust 1.92+ clippy lints in upstream copies | ✅ |
| **3c** | Deploy updated binary to local service | ✅ |

### 📊 Architecture (Current)

```
crates/
├── antigravity-types/          # 🔵 SINGLE SOURCE OF TRUTH (canonical definitions)
│   └── src/
│       ├── error/              # AccountError, ProxyError, ConfigError, TypedError
│       ├── models/             # Account, AppConfig, ProxyConfig, QuotaData, TokenData...
│       └── protocol/           # OpenAI/Claude/Gemini message types
├── antigravity-shared/         # 🟡 RE-EXPORT LAYER (no duplicates!)
│   └── src/
│       ├── lib.rs              # pub use antigravity_types::*;
│       ├── error.rs            # re-exports from types
│       ├── models/mod.rs       # re-exports from types
│       ├── proxy/config.rs     # re-exports from types
│       └── utils/              # HTTP utilities (re-export UpstreamProxyConfig)
├── antigravity-client/         # 🟣 RUST SDK (auto-discovery, retry, streaming)
│   └── src/
│       ├── client.rs           # AntigravityClient with auto_discover()
│       ├── error.rs            # ClientError enum
│       └── types.rs            # ChatRequest, ChatResponse, StreamChunk
├── antigravity-core/           # 🟢 BUSINESS LOGIC (all clippy-clean!)
│   └── src/proxy/
│       └── 23 modules          # ALL modules now clippy-clean
├── antigravity-server/         # 🔴 HTTP ENTRY POINT
vendor/
└── antigravity-upstream/       # Git submodule (REFERENCE ONLY)
```

### 🎯 Key Metrics

| Metric | Before | After |
|--------|--------|-------|
| Symlinks | 14 | **0** |
| Duplicate type definitions | ~20 | **0** |
| `#[allow(warnings)]` | 11 modules | **0** |
| Clippy warnings suppressed | ~58 | **0** |
| Unit tests | - | **112+** |
| Clippy status | ⚠️ | **✅ -D warnings** |
| Release build | - | **11MB** |

### ⏭️ Remaining Tasks

- [x] **Phase 4:** VPS deployment ✅ [2026-01-19] — `https://antigravity.quantumind.ru`
- [ ] **Phase 5:** Extract `antigravity-proxy` crate (optional cleanup)
- [x] **Phase 6:** CLI Management — full headless control without Web UI ✅ [2026-01-19]
- [x] **Phase 7:** Rust SDK (`antigravity-client`) — auto-discovery, retry, streaming ✅ [2026-01-19]
- [x] **Phase 7b:** Account auto-sync (60s interval) ✅ [2026-01-19]

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
cargo test -p antigravity-core --lib           # ✅ 149 tests pass
cargo build --release -p antigravity-server    # ✅ builds (1m 22s, 11MB)
```

---

## 📝 Changes Summary (2026-01-17)

### Phase 3c Completed

**Clippy cleanup in 11 formerly `#[allow(warnings)]` modules:**

1. **`src-leptos/`** — Fixed collapsible_if, clone_on_copy, unused_variables (9 fixes)
2. **`proxy/mappers/claude/utils.rs`** — Fixed if_same_then_else, unused_parens
3. **`proxy/mappers/claude/request.rs`** — Fixed manual_inspect (s→_s), unnecessary_unwrap
4. **`proxy/mappers/openai/request.rs`** — Fixed iter_cloned_collect→to_vec(), collapsible_match
5. **`proxy/mappers/tool_result_compressor.rs`** — Fixed manual_clamp
6. **`proxy/handlers/claude.rs`** — Fixed useless_vec (vec!→array literal)
7. Auto-fixed via `cargo clippy --fix`: ~40 lints (first() accessor, double-ended iter, etc.)

**All 23 proxy modules are now clippy-clean and pass `-D warnings`.**

### Modules Status

**ALL MODULES (22 total - clippy-clean):**
- `adaptive_limit`, `audio`, `common`, `handlers`, `health`, `mappers`, `middleware`
- `monitor`, `project_resolver`, `prometheus`, `providers`, `rate_limit`, `security`
- `server`, `session_manager`, `signature_cache`, `sticky_config`
- `token_manager`, `upstream`, `warp_isolation`, `zai_vision_mcp`, `zai_vision_tools`

> **Note:** `smart_prober` was removed on 2026-01-26 (dead code — never called).

---

## 🔀 UPSTREAM SYNC ARCHITECTURE [2026-01-18]

### Fork Strategy

This fork uses **SEMANTIC PORTING** — we don't blindly copy upstream files, we selectively integrate useful changes while maintaining our own improvements.

### Upstream Reference

- **Location:** `vendor/antigravity-upstream/` (git submodule)
- **Upstream repo:** https://github.com/lbjlaq/Antigravity-Manager
- **Current upstream:** v4.0.1
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

### Last Sync: 2026-01-26 (v4.0.1)

**Ported from v4.0.1:**
- **`gemini/collector.rs`** — NEW: Stream collector for Gemini SSE → JSON conversion
  - Collects streaming responses into complete JSON for non-stream requests
  - Signature caching side-effect during collection
  - Adjacent text part merging for optimization
- **`middleware/service_status.rs`** — NEW: Service status middleware (stub)
  - Placeholder for `is_running` state control (requires AppState extension)
- **`handlers/common.rs` RetryStrategy** — Unified retry logic
  - `RetryStrategy` enum: NoRetry, FixedDelay, LinearBackoff, ExponentialBackoff
  - `determine_retry_strategy()` with thinking signature error detection
  - `apply_retry_strategy()` async execution with logging
  - `should_rotate_account()` for 429/401/403/500 errors
- **`model_mapping.rs` improvements:**
  - `internal-background-task` → `gemini-2.5-flash` mapping for background tasks
  - Intelligent opus fallback → `gemini-3-pro-preview`
  - Multi-wildcard matching (`a*b*c` patterns)
- **`handlers/openai.rs` refactoring:**
  - Removed local RetryStrategy duplicate, now uses `super::common`

**OUR BUG FIXES (not in upstream):**
- **[FIX] 60s Global Lock missing rate limit check** (2026-01-26)
  - **Root cause:** TokenManager has 3 account selection modes:
    - Mode A: Sticky session (checks rate limit ✓)
    - Mode B: 60s global lock (MISSING rate limit check ✗)
    - Mode C: Round-robin (checks rate limit ✓)
  - **Symptom:** When account gets 429, Mode B still reuses it for 60 seconds because it only checked `attempted.contains()` and quota protection, NOT rate limit status.
  - **Result:** Infinite 429 loop — same account hammered for minutes despite being rate-limited.
  - **Fix:** Added `is_rate_limited()` check before reusing account in 60s window (line 637).
  - **Affected file:** `crates/antigravity-core/src/proxy/token_manager.rs`

- **[FIX] protected_models not populated in headless server** (2026-01-26)
  - **Root cause:** Headless server (`antigravity-server`) used `save_account()` after quota refresh, but this function does NOT check quota thresholds and does NOT populate `protected_models`. The correct function is `update_account_quota()` which contains the protection logic.
  - **Affected files:** `antigravity-server/src/api.rs`, `antigravity-server/src/commands.rs`
  - **Fix:** Replaced `save_account()` with `update_account_quota()` in:
    - `refresh_account_quota()` API handler
    - `refresh_all_quotas()` API handler
    - `refresh_quota()` CLI command
    - `refresh_all_quotas()` CLI command
  - **Additional fixes:** Fixed Rust 1.92 clippy warnings in `token_manager.rs`:
    - `collapsible_else_if` → collapsed nested else-if blocks
    - `map_or(false, ...)` → `is_some_and(...)`
  - **Important note:** Config is read from `~/.antigravity_tools/gui_config.json` (NOT `config.json`). The `quota_protection.enabled` must be `true` in this file for model protection to work.

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

**Ported from v3.3.49:**
- **`estimation_calibrator.rs`** — New module for token estimation calibration
  - Learns from actual API responses using exponential moving average
  - `record(estimated, actual)` → refines future predictions
  - `calibrate(estimated)` → applies learned correction factor
  - Global singleton via `OnceCell` for cross-request learning
- **[FIX #952] Nested `$defs` collection** — `collect_all_defs()` function
  - Recursively collects `$defs` from all schema levels
  - Fixes unresolved `$ref` fallback → converts to string type with hint
- **Stop sequences improvement** — removed from request transformation
  - Upstream removed `stop` field handling (models handle natively)
- **`common_utils.rs` OpenAI Image Parameters** — Extended API for image generation
  - `resolve_request_config()` now accepts `size: Option<&str>` and `quality: Option<&str>`
  - `parse_image_config_with_params()` — converts OpenAI size/quality to Gemini config
  - `calculate_aspect_ratio_from_size()` — "1024x1024" → "1:1", "1792x1024" → "16:9"
  - Quality mapping: "hd" → 4K, "medium" → 2K
- **`context_manager.rs` Multi-Language Token Estimation** — Improved accuracy
  - ASCII text: ~4 chars/token
  - CJK (Chinese, Japanese, Korean): ~1.5 chars/token
  - +15% safety margin for worst-case scenarios
  - Layer 1/2/3 compression hierarchy for thinking blocks

**Ported from v3.3.45:**
- **[FIX #820] Fixed Account Mode** — `preferred_account_id` in token_manager.rs
  - `set_preferred_account(Some(account_id))` — pins all requests to specific account
  - `set_preferred_account(None)` — returns to round-robin mode
  - Falls back to round-robin if preferred account is rate-limited or not found
- **ContextManager module** — Dynamic Thinking Stripping to prevent "Prompt is too long" and "Invalid signature" errors
  - `PurificationStrategy::None | Soft | Aggressive`
  - Token estimation based on 3.5 chars/token
  - Purifies history by removing old thinking blocks
- **SSE Peek Fix (Issue #859)** — Enhanced peek logic with:
  - Loop to skip heartbeat SSE comments (`:` prefix)
  - 60s timeout for first meaningful data (Claude), 30s for OpenAI
  - Retry on empty response or timeout during peek phase
  - **Applied to both `claude.rs` AND `openai.rs` handlers** (upstream only has it in claude.rs)
  - **[2026-01-20] OUR ENHANCEMENT:** Added total peek phase limits to prevent infinite hanging:
    - `MAX_PEEK_DURATION`: 120s (Claude) / 90s (OpenAI) — total time limit for peek phase
    - `MAX_HEARTBEATS`: 20 — limit on consecutive heartbeats without real data
    - If limits exceeded, request retries with account rotation (prevents client from hanging forever when model generates very large output)

**Ported from v3.3.43:**
- Shell command array fix (`local_shell_call` command → array)
- Thinking model signature handling (`skip_thought_signature_validator`)
- `clean_json_schema` for function call args
- `x-goog-api-key` header support in auth middleware
- Full `json_schema.rs` update (flatten_refs, merge_all_of, score_schema_option)
- `maxOutputTokens` default 64000 → 16384
- **[FIX #563]** `remaining_quota` field in `ProxyToken` + sorting by quota percentage
- **`start_auto_cleanup()`** — background task for expired rate limit cleanup (every 60s)
- **`reload_account()` / `reload_all_accounts()`** — hot-reload account configs
- **[FIX v3.3.36]** `close_tool_loop_for_thinking()` call after fallback retry — heals session to prevent "naked ToolResult" rejection
- **`is_retry` parameter** in `transform_claude_request_in()` — enables signature stripping on retry
- **`merge_consecutive_messages()`** — merges consecutive same-role messages for Gemini compatibility
- **`filter_invalid_thinking_blocks_with_family()`** — cross-model signature validation

**NOT ported (intentionally):**
- `protected_models` / quota protection system — requires `QuotaProtectionConfig` infrastructure that we don't have; our AIMD provides similar functionality
- `cli_sync.rs` module — Tauri-specific CLI config synchronization, not needed for headless server

**Our additions (not in upstream):**
- `tool_result_compressor` in OpenAI mapper (upstream only has it for Claude)
- AIMD predictive rate limiting
- Circuit breakers per account
- Prometheus metrics endpoint
- Resilience API endpoints
- WARP proxy support for per-account IP isolation (`call_v1_internal_with_warp`)
- **Sticky session rebind on 429** — preserves prompt cache after rate limit failover (see below)

**Dead Code Cleanup (2026-01-26):**
- **`smart_prober.rs`** — DELETED (entire module, 14 pub functions, never called from anywhere)
- **`prometheus.rs`** — Removed 6 dead functions:
  - `record_log_rotation`, `record_log_cleanup`, `record_adaptive_probe`
  - `record_hedge_win`, `record_primary_win`, `update_adaptive_limit_gauge`
- **`src-tauri/`** — DELETED (6.5MB obsolete v3.3.20 copy, real upstream is `vendor/antigravity-upstream/` v4.0.1)
- **Commit:** `89abe947` — 154 files changed, 26,994 lines deleted

**API Architecture Fixes (2026-01-26):**
- **Concurrent batch operations** — `refresh_all_quotas`, `warmup_all_accounts`, `add_account_by_token` now use `JoinSet` for parallel execution instead of sequential loops
- **OAuth CSRF protection** — Added `state` parameter generation and validation in OAuth flow (`generate_oauth_state`, `validate_oauth_state` in AppState)
- **Port resolution from AppState** — OAuth redirect URI now uses actual server port from `AppState::get_proxy_port()` instead of `ANTIGRAVITY_PORT` env var
- **Error logging in batch operations** — Failed operations now log specific error messages via `tracing::warn!`

---

## ✅ FIX: Sticky Session Rebind on 429 [2026-01-19]

### The Problem (Both Upstream & Fork Had This Bug)

When a 429 rate limit triggers account switch, the session was NOT rebound to the new account:

```
1. Session X → Account A (bound via session_accounts map)
2. Request fails with 429 → token_manager switches to Account B
3. Session X still bound to Account A (BUG!)
4. Next request → system might return to Account A (if recovered)
5. Result: Prompt cache broken on BOTH accounts
```

Google caches prompts per `project_id`. Each account has unique project (e.g., `optimum-cell-kvmxc`, `original-diagram-4l9f4`). Switching back and forth destroys cache continuity.

### The Fix

Added central rebind logic in `token_manager.rs` (lines 651-671) after token selection:

```rust
// After token is selected, ensure session is bound to it
if let Some(sid) = session_id {
    if scheduling.mode != SchedulingMode::PerformanceFirst {
        let current_binding = self.session_accounts.get(sid).map(|v| v.clone());
        if current_binding.as_ref() != Some(&token.account_id) {
            self.session_accounts.insert(sid.to_string(), token.account_id.clone());
            tracing::debug!(
                "[Session Rebind] {} rebound: {:?} → {}",
                sid, current_binding, token.account_id
            );
        }
    }
}
```

This covers ALL token selection paths:
- **Mode A (Cache First):** Existing binding → fallback on 429 → rebind
- **Mode B (Balance):** Least-used selection → rebind if different
- **Mode C (Rotation):** Round-robin → rebind on each request
- **60s optimistic reset:** When rate limit expires → rebind to recovered account

### Why This Matters

- **Prompt cache preserved:** Session stays on new account, cache builds there
- **No ping-pong:** Session doesn't return to original account after 429
- **Upstream still has this bug:** They don't rebind after failover

### Verification

```bash
# Watch for rebind logs
journalctl --user -u antigravity-manager -f | grep "Session Rebind"
```

---

## ⚠️ KNOWN ARCHITECTURAL QUIRK: Shared Project Rate Limits [2026-01-18]

### The Issue

Rate limits are tracked per **account_id**, but Google Cloud quotas are enforced per **project_id**.

If two accounts share the same Google Cloud Project:
1. Account A gets 429 → marked as rate-limited
2. System switches to Account B (same project)
3. Account B immediately gets 429 (shared project quota)
4. System incorrectly considers B as "fresh" account

### Current Implementation (Both Upstream & Fork)

```rust
// rate_limit.rs
pub struct RateLimitTracker {
    limits: DashMap<String, RateLimitInfo>,  // Key = account_id, NOT project_id
}
```

The `project_id` is only used in API request payloads, not in rate limit tracking.

### Why We DON'T Fix This (Yet)

**Prompt caching benefit:** Google's prompt caching is tied to `project_id`. If we start tracking rate limits per project and avoiding all accounts in a rate-limited project, we might break the caching optimization that upstream designed around.

The current behavior may be intentional — when one account hits 429, switching to another account in the same project might still benefit from cached prompts, and the 429 on the second account could be shorter.

### Potential Future Fix

If caching proves not valuable for our use case:

```rust
// Add project-level tracking:
project_limits: DashMap<String, RateLimitInfo>  // project_id → info

fn is_rate_limited(&self, account_id: &str, project_id: &str) -> bool {
    self.limits.get(account_id).is_some() 
    || self.project_limits.get(project_id).is_some()
}
```

### How to Verify Shared Project

```bash
cat ~/.antigravity_tools/accounts/*.json | jq -r '.token.project_id' | sort | uniq -c
```

If multiple accounts show the same project_id, they share quota.

---

## 🔍 BACKEND DISCOVERY: Model Routing [2026-01-18]

### What Google Antigravity Actually Is

**Google Antigravity** (antigravity.google) is Google's new AI IDE — a competitor to Cursor/Windsurf.

Antigravity Manager exploits the API that powers this IDE:

```
Your Client (OpenCode, Cursor, etc.)
    ↓
Antigravity Manager (localhost:8045)
    ↓ pretends to be Antigravity IDE client
Google Antigravity API (antigravity.google)
    ↓
Backend (Gemini / Claude via Vertex)
```

### Model Backend Discovery (Verified 2026-01-18)

Tested by asking models "What model are you?":

| Model Alias | Actual Backend | Evidence |
|-------------|----------------|----------|
| `gpt-4o`, `gpt-4o-mini`, `gpt-*` | **Gemini** (alias) | Responds: "I am gemini-1.5-flash-pro" |
| `gemini-3-pro`, `gemini-*` | **Gemini** (native) | Responds with Antigravity system prompt |
| `claude-opus-4-5`, `claude-*` | **Claude via Vertex AI** | Error contains `req_vrtx_*` request ID |

### Key Insights

1. **GPT models are fake** — they're just Gemini with OpenAI-compatible response format
2. **Claude models are REAL** — Google has Vertex AI partnership with Anthropic, routes to actual Claude
3. **Why GPT aliases exist** — Backend is shared with AI Studio/Vertex which supports OpenAI format for migration ease

### Why Google Allows This

- Antigravity IDE = user acquisition strategy (compete with Cursor)
- Free tier attracts developers → converts to paid Vertex AI enterprise
- Market share now, monetization later
- Rate limits are their protection (Antigravity Manager rotates accounts to bypass)

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

### Future Fix Ideas

1. **Auto-continue in proxy** — detect truncated stream (no valid stop_reason), auto-send "continue" request, splice responses
2. **Output size estimation** — before sending request, estimate expected output size, warn if >4K tokens
3. **Paid API fallback** — route large-output requests to OpenRouter/direct Anthropic API

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

```bash
# 1. Build new binary (includes frontend via build.rs)
cargo build --release -p antigravity-server

# 2. Start new instance (binds alongside old via SO_REUSEPORT)
ANTIGRAVITY_STATIC_DIR=... ~/.local/bin/antigravity-server.new &
sleep 3  # Wait for initialization

# 3. Stop old instance (graceful drain)
systemctl --user stop antigravity-manager

# 4. Replace binary
mv ~/.local/bin/antigravity-server.new ~/.local/bin/antigravity-server

# 5. Start via systemd
systemctl --user start antigravity-manager
```

Or use: `./scripts/zero-downtime-deploy.sh`

### Important: Unified Build

**Backend and frontend are built together** via `build.rs`:

```rust
// antigravity-server/build.rs
// Automatically runs `trunk build` when compiling server
```

This means `cargo build -p antigravity-server` builds BOTH:
- Rust backend binary
- Leptos WASM frontend (via trunk)

**DO NOT deploy backend without rebuilding frontend** — they share the same release cycle.

### Systemd Configuration

```ini
# ~/.config/systemd/user/antigravity-manager.service
[Service]
ExecStart=/home/stranmor/.local/bin/antigravity-server
TimeoutStopSec=35  # Allow graceful drain
Restart=always
```

Socket activation (`antigravity-manager.socket`) is **disabled** — SO_REUSEPORT replaces it.

---

## 📦 BUILD SYSTEM [2026-01-19]

### Unified Build Architecture

```
cargo build -p antigravity-server
    ↓
build.rs executes
    ↓
trunk build (compiles Leptos → WASM)
    ↓
WASM artifacts → src-leptos/dist/
    ↓
Server binary embeds path to dist/
```

### Why Unified Build Matters

1. **Atomic deploys** — frontend and backend always match
2. **No forgotten rebuilds** — one command builds everything
3. **Version consistency** — both use same git commit

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

No conflicts. Eventual consistency guaranteed.

### Data Structures

```rust
// crates/antigravity-types/src/models/sync.rs

pub struct MappingEntry {
    pub target: String,      // e.g., "gemini-3-pro-high"
    pub updated_at: i64,     // Unix timestamp (ms)
}

pub struct SyncableMapping {
    pub entries: HashMap<String, MappingEntry>,
    pub instance_id: Option<String>,
}
```

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

### Files

- `crates/antigravity-types/src/models/sync.rs` — `SyncableMapping`, `MappingEntry`, LWW merge logic
- `antigravity-server/src/config_sync.rs` — Auto-sync background task
- `antigravity-server/src/state.rs` — `get_syncable_mapping()`, `merge_remote_mapping()`
- `antigravity-server/src/api.rs` — `/api/config/mapping` endpoints

