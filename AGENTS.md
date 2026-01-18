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

- [ ] **Phase 4:** VPS deployment (optional)
- [ ] **Phase 5:** Extract `antigravity-proxy` crate (optional cleanup)

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
cargo test -p antigravity-core --lib           # ✅ 112+ tests pass
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

**ALL MODULES (23 total - clippy-clean):**
- `adaptive_limit`, `audio`, `common`, `handlers`, `health`, `mappers`, `middleware`
- `monitor`, `project_resolver`, `prometheus`, `providers`, `rate_limit`, `security`
- `server`, `session_manager`, `signature_cache`, `smart_prober`, `sticky_config`
- `token_manager`, `upstream`, `warp_isolation`, `zai_vision_mcp`, `zai_vision_tools`

---

## 🔀 UPSTREAM SYNC ARCHITECTURE [2026-01-18]

### Fork Strategy

This fork uses **SEMANTIC PORTING** — we don't blindly copy upstream files, we selectively integrate useful changes while maintaining our own improvements.

### Upstream Reference

- **Location:** `vendor/antigravity-upstream/` (git submodule)
- **Upstream repo:** https://github.com/lbjlaq/Antigravity-Manager
- **Current upstream:** v3.3.43
- **Our version:** v3.3.20 (with custom improvements)

### Intentional Divergences

| File | Lines Diff | Reason |
|------|------------|--------|
| `handlers/claude.rs` | ~1500 | **OUR ADDITIONS:** AIMD rate limiting, resilience patterns, Axum-specific handlers, circuit breakers |
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

### Last Sync: 2026-01-18

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
