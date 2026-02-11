# Tech Debt Remediation — High & Medium Severity Issues

## TL;DR

> **Quick Summary**: Systematic remediation of 28 confirmed unfixed High/Medium severity issues from the Known Issues table, organized into 9 work packages by subsystem and dependency order.
> 
> **Deliverables**:
> - 9 work packages fixing race conditions, data loss, blocking I/O, correctness bugs, and traffic leak risks
> - AGENTS.md updated to mark 6 issues as already-fixed (discovered during research)
> 
> **Estimated Effort**: Large (9 packages, ~35 files)
> **Parallel Execution**: YES — 3 waves
> **Critical Path**: WP1 (state race conditions) → WP4 (hot-reload state loss) → WP9 (AGENTS.md cleanup)

---

## Context

### Original Request
Analyze all unfixed High and Medium severity issues from the AGENTS.md Known Issues table, read the actual source code, group into logical work packages, and produce a prioritized implementation plan.

### Research Summary

**6 explore agents** read all relevant source files. Key findings:

**Already Fixed (AGENTS.md is stale — mark as resolved):**
- `token_manager/selection.rs` — `get_token_forced` DOES check expiry + project ID (lines 82-89)
- `token_manager/selection_helpers.rs` — sort comparator DOES pre-fetch into Vec (lines 110-112, 206-210)
- `token_manager/token_refresh.rs` — `refresh_locks` ARE cleaned every 60s in `start_auto_cleanup` (line 158)
- `tool_result_compressor/mod.rs` — regexes use `OnceLock` (lines 19-39), compiled once
- `zai_anthropic.rs:deep_remove_cache_control` — info logging already removed
- `account_pg_targeted.rs` — 1:1 PK constraint means no session overwrite. `project_id` in tokens is semantic misplacement but functionally correct

**Confirmed Unfixed: 28 issues across 9 work packages**

---

## Work Objectives

### Core Objective
Fix all High and Medium severity tech debt issues, prioritized: safety-critical → correctness → performance → code quality.

### Must Have
- All race conditions eliminated or mitigated with documented tradeoffs
- All blocking I/O wrapped in `spawn_blocking` or replaced with async equivalents
- All silent error swallowing replaced with proper error propagation
- All traffic leak risks gated with explicit failure modes

### Must NOT Have (Guardrails)
- Breaking changes to the external API contract (OpenAI/Claude compatible endpoints)
- Changes that require restarting client sessions
- Removal of JSON fallback paths (still needed for non-PostgreSQL setups)
- New dependencies unless strictly necessary (prefer stdlib/tokio solutions)

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (344 unit tests, 1 integration test)
- **Automated tests**: YES (tests-after — each WP adds tests for changed behavior)
- **Framework**: `cargo test`

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately — no dependencies):
├── WP1: State Race Conditions (High — safety-critical)
├── WP2: Blocking I/O Elimination (High — executor starvation)
├── WP3: Claude Handler Retry & Error Correctness (Medium — correctness)
└── WP6: Content Processing Fixes (High/Medium — blocking I/O + correctness)

Wave 2 (After Wave 1):
├── WP4: Proxy Server Hot-Reload State Loss (Medium — depends on WP1 state patterns)
├── WP5: Upstream Client Traffic Leak (Medium — touches client_builder from WP2)
└── WP7: OpenAI/Codex Stream Correctness (Medium — independent subsystem)

Wave 3 (After Wave 2):
├── WP8: Miscellaneous Medium Issues (cleanup)
└── WP9: AGENTS.md Accuracy Update (documentation)
```

### Dependency Matrix

| WP | Depends On | Blocks | Can Parallelize With |
|----|------------|--------|---------------------|
| 1 | None | 4 | 2, 3, 6 |
| 2 | None | 5 | 1, 3, 6 |
| 3 | None | None | 1, 2, 6 |
| 4 | 1 | None | 5, 7 |
| 5 | 2 | None | 4, 7 |
| 6 | None | None | 1, 2, 3 |
| 7 | None | None | 4, 5 |
| 8 | None | None | 4, 5, 7 |
| 9 | ALL | None | None (final) |

---

## TODOs

---

- [ ] 1. WP1: State Race Conditions & Error Swallowing (SAFETY-CRITICAL)

  **Issues addressed:**
  - `state/accessors.rs` — **[High]** `hot_reload_proxy_config` updates 6 RwLocks sequentially (non-atomic)
  - `state/accessors.rs` — **[Medium]** `get_model_quota` uses `.contains()` instead of `.starts_with()`
  - `state/accessors.rs` — **[Medium]** `switch_account` updates file before repository (split-brain)
  - `state/accessors.rs` — **[Medium]** `get_account_count` swallows DB errors, returns 0
  - `state/accessors.rs` — **[Medium]** `load_config()` sync file I/O in async context
  - `modules/account/fetch.rs` — **[High]** Race condition: concurrent fetches cause lost updates (JSON path)
  - `token_manager/mod.rs` — **[Medium]** Arbitrary session eviction (hash order, not LRU) + race in `active_requests.retain`

  **Files to modify:**
  - `antigravity-server/src/state/accessors.rs`
  - `crates/antigravity-types/src/models/quota.rs` (line 101-104)
  - `crates/antigravity-core/src/modules/account/fetch.rs`
  - `crates/antigravity-core/src/proxy/token_manager/mod.rs`

  **Fix approach:**

  **1a. `hot_reload_proxy_config` — atomic config swap:**
  Current code (lines 102-139) acquires/releases 6 RwLocks in separate scopes. Fix: acquire ALL write guards in a single scope before mutating any field. Order must be deterministic to prevent deadlock (alphabetical: `custom_mapping`, `experimental_config`, `proxy_config`, `security_config`, `upstream_proxy`, `zai_config`).
  ```rust
  // Acquire all locks simultaneously
  let mut map_guard = self.custom_mapping.write().await;
  let mut exp_guard = self.experimental_config.write().await;
  let mut proxy_guard = self.proxy_config.write().await;
  let mut sec_guard = self.security_config.write().await;
  let mut upstream_guard = self.upstream_proxy.write().await;
  let mut zai_guard = self.zai_config.write().await;
  // Now mutate all, then drop all guards at once
  ```

  **1b. `get_model_quota` — `.contains()` → `.starts_with()`:**
  In `quota.rs` line 103, replace `.contains(&prefix_lower)` with `.starts_with(&prefix_lower)`.

  **1c. `switch_account` — reverse operation order:**
  Lines 47-57: Move DB write (`repo.set_current_account_id`) BEFORE file write (`account::switch_account`). If DB fails, file is untouched — no split-brain. If file fails after DB, `get_current_account` reads repo-first so the DB value is authoritative.

  **1d. `get_account_count` — propagate errors:**
  Change return type from `usize` to `Result<usize, String>`. Remove `.ok()` calls. Callers must handle the error (update call sites — search for `get_account_count` usages).

  **1e. `load_config()` in async context:**
  Wrap the `load_config()` call inside `hot_reload_proxy_config` in `tokio::task::spawn_blocking()`. The function already returns `Result`, so spawn_blocking + await integrates cleanly.

  **1f. `fetch.rs` JSON fallback race:**
  Add per-account `Mutex` (keyed by `account_id`) around the read-modify-write cycle in `persist_quota_data` and `persist_token_data` (JSON path only). Use a `DashMap<String, Arc<Mutex<()>>>` stored alongside the function or passed as parameter. The PostgreSQL path uses targeted SQL updates and is already safe.

  **1g. Session eviction — LRU ordering:**
  Change `session_accounts` from `DashMap<String, String>` to `DashMap<String, (String, Instant)>`. On bind, store `Instant::now()`. In eviction (`start_auto_cleanup`), collect all entries, sort by timestamp ascending, evict oldest instead of `.take()` on hash order.

  **1h. `active_requests.retain` race:**
  The race is benign (ActiveRequestGuard is authoritative), but the cleanup can be made safer: instead of `retain(|_, v| v.load(Relaxed) > 0)`, use `retain(|_, v| v.load(Acquire) > 0)` for memory ordering, and accept that rare phantom entries are harmless (bounded by account count).

  **Risk level:** Medium-High. `hot_reload_proxy_config` lock ordering must be correct to avoid deadlocks. `get_account_count` signature change requires updating all callers. Session eviction change alters `session_accounts` value type.

  **Recommended Agent Profile:**
  - **Category**: `ultrabrain`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with WP2, WP3, WP6)
  - **Blocks**: WP4, WP9
  - **Blocked By**: None

  **Acceptance Criteria:**
  - [ ] `hot_reload_proxy_config` acquires all 6 write guards in single scope
  - [ ] `get_model_quota` uses `.starts_with()` — test: `get_model_quota("gem")` does NOT match "anti-gem-v2"
  - [ ] `switch_account` writes DB first, file second
  - [ ] `get_account_count` returns `Result<usize, String>` — all callers updated
  - [ ] `load_config()` call wrapped in `spawn_blocking`
  - [ ] `persist_quota_data` JSON path has per-account mutex
  - [ ] Session eviction evicts oldest by timestamp, not by hash order
  - [ ] `cargo test --workspace` passes
  - [ ] `cargo clippy --workspace -- -Dwarnings` passes

  **Commit**: YES
  - Message: `fix(state): eliminate race conditions in config reload, account switching, and session eviction`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 2. WP2: Blocking I/O Elimination (EXECUTOR STARVATION)

  **Issues addressed:**
  - `modules/proxy_db.rs` — **[High]** Blocking SQLite I/O via `thread_local` rusqlite on async runtime
  - `antigravity-server/src/api/` — **[Medium]** Blocking file I/O in 14+ async handler call sites
  - `proxy/middleware/monitor.rs` — **[Medium]** 2MB buffer per connection under high concurrency (memory pressure)

  **Files to modify:**
  - `crates/antigravity-core/src/modules/proxy_db.rs`
  - `antigravity-server/src/api/config.rs`
  - `antigravity-server/src/api/device.rs`
  - `antigravity-server/src/api/proxy.rs`
  - `antigravity-server/src/api/quota/warmup.rs`
  - `antigravity-server/src/api/quota/refresh.rs`
  - `antigravity-server/src/api/accounts.rs`
  - `antigravity-server/src/api/oauth.rs`
  - `crates/antigravity-core/src/proxy/middleware/monitor.rs`

  **Fix approach:**

  **2a. `proxy_db.rs` — wrap SQLite in `spawn_blocking`:**
  The `with_connection` function (line 26) runs synchronous SQLite operations. Wrap each public function (`save_log`, `get_logs`, `get_log_detail`, `get_log_count`, `cleanup_old_logs`) in `tokio::task::spawn_blocking()`. The `thread_local!` Connection pattern works inside spawn_blocking since those threads can have their own TLS. Return `Result` from the spawned closure and propagate.

  **2b. API handlers — wrap file operations in `spawn_blocking`:**
  Each blocking call site (14 identified: config.rs:14, config.rs:24, device.rs:13-16, device.rs:29-36, proxy.rs:51-53, warmup.rs:39, warmup.rs:60, warmup.rs:134, refresh.rs:28, refresh.rs:77, oauth.rs:193, oauth.rs:272, accounts.rs:118, accounts.rs:139) must be wrapped in `spawn_blocking`. Pattern:
  ```rust
  let result = tokio::task::spawn_blocking(move || {
      core_config::load_config()
  }).await.map_err(|e| /* JoinError handling */)?;
  ```
  For handlers that chain multiple blocking calls, group them into a single `spawn_blocking` block.

  **2c. `monitor.rs` — reduce buffer per connection:**
  Line 57: reduce 2MB limit to 512KB for request body buffering. Line 240: same for response. 512KB captures 99%+ of API request/response bodies while reducing worst-case memory from 4MB/conn to 1MB/conn. Add a counter metric (`gauge!("monitor_active_buffers")`) to track active buffer count.

  **Risk level:** Low-Medium. `spawn_blocking` is mechanical wrapping. Monitor buffer reduction may truncate unusually large request bodies (only affects logging, not request processing).

  **Recommended Agent Profile:**
  - **Category**: `unspecified-high`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with WP1, WP3, WP6)
  - **Blocks**: WP5
  - **Blocked By**: None

  **Acceptance Criteria:**
  - [ ] All `proxy_db.rs` public functions run inside `spawn_blocking`
  - [ ] All 14 API handler blocking call sites wrapped in `spawn_blocking`
  - [ ] Monitor request body buffer reduced to 512KB
  - [ ] `cargo test --workspace` passes
  - [ ] `cargo clippy --workspace -- -Dwarnings` passes

  **Commit**: YES
  - Message: `fix(io): wrap all blocking file/SQLite I/O in spawn_blocking, reduce monitor buffer to 512KB`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 3. WP3: Claude Handler Retry & Error Correctness

  **Issues addressed:**
  - `proxy/handlers/claude/messages.rs` — **[Medium]** Grace retry unconditionally increments `attempt`, consuming budget. Network errors not added to `attempted_accounts`. Transport errors mapped to `TokenAcquisition` instead of `ConnectionError`.
  - `proxy/handlers/claude/streaming.rs` — **[Medium]** Error handler injects `content_block_start` at hardcoded `index: 0` (duplicate index). `record_stream_graceful_finish` called on error path (misleading metrics).

  **Files to modify:**
  - `crates/antigravity-core/src/proxy/handlers/claude/messages.rs`
  - `crates/antigravity-core/src/proxy/handlers/claude/streaming.rs`

  **Fix approach:**

  **3a. `messages.rs` — separate grace retry counter from attempt budget:**
  Line 87/269: Add a separate `grace_attempt` counter. Only increment `attempt` for non-grace retries. Grace retries use their own budget (already limited to 1 via `grace_retry_used` flag) but should not consume the main retry budget.

  **3b. `messages.rs` — add network errors to `attempted_accounts`:**
  Line 176-181: When `Err(e)` from `call_v1_internal_with_warp`, add the current account's email to `attempted_accounts` so the next iteration won't re-select the same failing account:
  ```rust
  Err(e) => {
      if let Some(ref email) = current_email {
          attempted_accounts.insert(email.clone());
      }
      // ... rest of error handling
  }
  ```

  **3c. `messages.rs` — correct error type for transport errors:**
  Line 180: Replace `UpstreamError::TokenAcquisition(e.to_string())` with `UpstreamError::ConnectionError(e.to_string())` (or the appropriate variant that semantically represents a transport failure).

  **3d. `streaming.rs` — dynamic content_block index:**
  Line 103: Instead of hardcoded `"index": 0`, track the last emitted block index from the successful portion of the stream and use `last_index + 1`. Pass the block index tracker through the streaming state.

  **3e. `streaming.rs` — remove misleading metric on error path:**
  Line 101: Remove or replace `record_stream_graceful_finish("claude")` on the error path. This is an abort, not a graceful finish. Either: remove the metric call entirely, or use a distinct metric like `record_stream_abort("claude")`.

  **Risk level:** Low. Changes are localized to error handling paths. No impact on happy path.

  **Recommended Agent Profile:**
  - **Category**: `unspecified-low`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with WP1, WP2, WP6)
  - **Blocks**: None
  - **Blocked By**: None

  **Acceptance Criteria:**
  - [ ] Grace retries do NOT increment `attempt` counter
  - [ ] Network errors add account to `attempted_accounts`
  - [ ] Transport errors use `ConnectionError` variant, not `TokenAcquisition`
  - [ ] Error-injected `content_block_start` uses dynamic index (not hardcoded 0)
  - [ ] No `record_stream_graceful_finish` on error/abort paths
  - [ ] `cargo test --workspace` passes
  - [ ] `cargo clippy --workspace -- -Dwarnings` passes

  **Commit**: YES
  - Message: `fix(claude): correct retry budget accounting, error classification, and stream error handling`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 4. WP4: Proxy Server Hot-Reload State Loss

  **Issues addressed:**
  - `proxy/server.rs` — **[Medium]** `provider_rr` and `zai_vision_mcp_state` recreated on each `build_proxy_router_with_shared_state` call — state lost on hot-reload
  - `proxy/server.rs` — **[Medium]** `UpstreamClient` initialized with `None` for circuit_breaker — circuit breaking disabled for upstream
  - `state/mod.rs` — **[Medium]** `health_monitor` shared instance vs separate instances in `token_manager`

  **Files to modify:**
  - `crates/antigravity-core/src/proxy/server.rs`
  - `antigravity-server/src/state/mod.rs`

  **Fix approach:**

  **4a. `server.rs` — persist `provider_rr` and `zai_vision_mcp_state` across hot-reloads:**
  Lines 52-53: Move `provider_rr` (round-robin provider index) and `zai_vision_mcp_state` out of `build_proxy_router_with_shared_state`. Make them fields of the shared state struct (or pass as parameters). On hot-reload, the existing instances are reused instead of recreated.

  Concrete approach: Add `provider_rr: Arc<AtomicUsize>` and `zai_vision_mcp_state: Arc<ZaiVisionMcpState>` (or equivalent) to `AppState`. Initialize once in `state/mod.rs`, pass into `build_proxy_router_with_shared_state`.

  **4b. `server.rs` — enable circuit breaker for upstream:**
  Line 64: Replace `None` with a configured `CircuitBreakerConfig`. Use the same config pattern as the per-account circuit breakers. Default: 5 failures → open for 30s → half-open → probe.

  **4c. `state/mod.rs` — verify `health_monitor` sharing:**
  Lines 69, 87-90: Verify that `token_manager.set_health_monitor(health_monitor.clone())` correctly shares the SAME instance (not creating a new one internally). If `TokenManager` creates its own internally, remove that and use only the passed-in one. Also: move `health_monitor.start_recovery_task()` to AFTER `set_health_monitor()` so recovery task can report to token_manager from the start.

  **Risk level:** Medium. Moving state out of the router builder changes the function signature. Circuit breaker enablement changes upstream failure behavior.

  **Recommended Agent Profile:**
  - **Category**: `unspecified-high`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with WP5, WP7)
  - **Blocks**: None
  - **Blocked By**: WP1 (state/mod.rs patterns)

  **Acceptance Criteria:**
  - [ ] `provider_rr` and `zai_vision_mcp_state` survive hot-reload (persist across router rebuilds)
  - [ ] Upstream circuit breaker is enabled with default 5-failure / 30s-open config
  - [ ] Single `HealthMonitor` instance shared between state and token_manager
  - [ ] `start_recovery_task()` called after `set_health_monitor()`
  - [ ] `cargo test --workspace` passes

  **Commit**: YES
  - Message: `fix(server): persist routing state across hot-reloads, enable upstream circuit breaker`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 5. WP5: Upstream Client Traffic Leak & Client Builder

  **Issues addressed:**
  - `upstream/client/mod.rs` — **[Medium]** `get_client()` silently falls back to direct connection on proxy build failure — traffic leak risk
  - `proxy/common/client_builder.rs` — **[Medium]** Returns `Result<_, String>`, silent 5s timeout clamp, empty proxy URL with `enabled: true` silently skips proxy

  **Files to modify:**
  - `crates/antigravity-core/src/proxy/upstream/client/mod.rs`
  - `crates/antigravity-core/src/proxy/common/client_builder.rs`

  **Fix approach:**

  **5a. `upstream/client/mod.rs` — fail loudly on proxy build failure:**
  Lines 93-99 and 117-122: When proxy config is `enabled: true` but client build fails, log at `error!` level (not silent), and return `Err` instead of falling back to direct connection. Callers must handle the error. The direct connection fallback should only be used when proxy is explicitly `enabled: false`.

  Lines 126-128: Same for `spawn_blocking` panic — propagate the error instead of falling back to direct.

  **5b. `client_builder.rs` — typed error + log warnings:**
  Line 8: Replace `Result<_, String>` with `Result<_, ClientBuildError>` where `ClientBuildError` is an enum: `InvalidTimeout`, `ProxyConfigError(String)`, `BuildError(reqwest::Error)`.

  Line 10: Log `warn!("Timeout {} clamped to minimum 5s", timeout_secs)` when clamp activates.

  Line 17: When `config.enabled && config.url.is_empty()`, return `Err(ClientBuildError::ProxyConfigError("proxy enabled but URL is empty"))` instead of silently skipping.

  **Risk level:** Medium. Changing fallback behavior from silent success to explicit failure may surface hidden issues in environments where proxy config is accidentally enabled. But that's the correct behavior — silent traffic leak is worse.

  **Recommended Agent Profile:**
  - **Category**: `unspecified-low`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with WP4, WP7)
  - **Blocks**: None
  - **Blocked By**: WP2 (client_builder.rs may be touched)

  **Acceptance Criteria:**
  - [ ] Proxy build failure with `enabled: true` returns `Err`, not silent direct fallback
  - [ ] `client_builder.rs` returns typed `ClientBuildError` enum
  - [ ] Empty proxy URL with `enabled: true` returns explicit error
  - [ ] Timeout clamp logs warning
  - [ ] `cargo test --workspace` passes

  **Commit**: YES
  - Message: `fix(upstream): fail explicitly on proxy build errors instead of silent direct connection fallback`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 6. WP6: Content Processing Fixes (Blocking I/O + Correctness)

  **Issues addressed:**
  - `proxy/mappers/openai/request/content_parts.rs` — **[High]** Blocking file reads, missing percent-decoding for `file://` paths, unbounded memory for large videos
  - `proxy/common/json_schema/recursive.rs` — **[Medium]** Unbounded recursion in schema cleaning can overflow stack
  - `proxy/mappers/claude/request/generation_config.rs` — **[Medium]** `claude_req.stop_sequences` silently ignored — hardcoded `stopSequences`

  **Files to modify:**
  - `crates/antigravity-core/src/proxy/mappers/openai/request/content_parts.rs`
  - `crates/antigravity-core/src/proxy/common/json_schema/recursive.rs`
  - `crates/antigravity-core/src/proxy/mappers/claude/request/generation_config.rs`

  **Fix approach:**

  **6a. `content_parts.rs` — async file reads + percent-decoding + size limit:**
  Lines 67, 132: Replace `std::fs::read()` with `tokio::fs::read()` (the calling context is already async).

  Lines 52-63, 117-128: Add `percent_decode_str()` from the `percent-encoding` crate to decode `file://` URL paths before filesystem access.

  Add size check before reading: `if metadata.len() > MAX_FILE_SIZE { return Err(...) }`. Set `MAX_FILE_SIZE = 100 * 1024 * 1024` (100MB) to prevent OOM on huge video files. Return error to client instead of silently loading GBs.

  **6b. `recursive.rs` — add depth limit:**
  Add `depth: usize` parameter to `clean_json_schema_recursive`. At each recursive call, pass `depth + 1`. When `depth > MAX_SCHEMA_DEPTH` (e.g., 64), return `false` without recursing further. This is a defensive limit — no valid JSON schema should nest 64 levels deep.

  Since this is `pub(super)`, the caller in the parent module needs to pass `depth: 0` at the initial call.

  **6c. `generation_config.rs` — respect client stop_sequences:**
  Line 118: Instead of hardcoded `stopSequences`, merge client-provided values with the default set. Start with the 3 default stop sequences already hardcoded, then extend with any `claude_req.stop_sequences` the client provided (if not None/empty). Deduplicate via `HashSet` or by checking before insert. Cap total at 5 (Gemini API limit).

  **Risk level:** Medium for content_parts.rs (changing sync to async may require callers to be async). Low for recursive.rs and generation_config.rs.

  **Recommended Agent Profile:**
  - **Category**: `unspecified-high`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with WP1, WP2, WP3)
  - **Blocks**: None
  - **Blocked By**: None

  **Acceptance Criteria:**
  - [ ] `content_parts.rs` uses `tokio::fs::read()` instead of `std::fs::read()`
  - [ ] `file://` paths are percent-decoded before filesystem access
  - [ ] Files >100MB return error instead of being loaded into memory
  - [ ] `clean_json_schema_recursive` has depth limit (64), does not stack overflow on deep schemas
  - [ ] Client-provided `stop_sequences` are merged with defaults, not discarded
  - [ ] `cargo test --workspace` passes
  - [ ] `cargo clippy --workspace -- -Dwarnings` passes

  **Commit**: YES
  - Message: `fix(mappers): async file reads, schema depth limit, preserve client stop_sequences`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 7. WP7: OpenAI/Codex Stream Correctness

  **Issues addressed:**
  - `proxy/handlers/openai/chat/stream_handler.rs` — **[Medium]** `build_combined_stream` graceful finish applies to both streaming and non-streaming paths — `collect_to_json` may interpret mid-stream failures as partial success
  - `proxy/mappers/openai/streaming/codex_stream.rs` — **[Medium]** Smart quote replacement corrupts content. `emitted_tool_calls` dedup suppresses valid repeated calls. `usageMetadata` extracted but discarded.

  **Files to modify:**
  - `crates/antigravity-core/src/proxy/handlers/openai/chat/stream_handler.rs`
  - `crates/antigravity-core/src/proxy/mappers/openai/streaming/codex_stream.rs`

  **Fix approach:**

  **7a. `stream_handler.rs` — gate graceful finish on streaming mode:**
  The `build_combined_stream` function (lines 69-89) should receive a `client_wants_stream: bool` parameter. When `false` (non-streaming / collect_to_json path), mid-stream errors should propagate as `Err` instead of being converted to graceful `finish_reason: "length"`. This ensures `collect_openai_stream_to_json` correctly returns `Retry` on errors instead of interpreting graceful-finish chunks as a valid complete response.

  **7b. `codex_stream.rs` — remove smart quote replacement:**
  Line 93: Remove `text.replace(['\u201c', '\u201d'], "\"")`. Smart quotes in generated content are intentional — replacing them corrupts typography and code that legitimately uses these characters.

  **7c. `codex_stream.rs` — fix tool call dedup:**
  Lines 110-116: Replace full JSON serialization dedup with a semantic dedup that uses `(function_name, call_id)` as the key, not the full JSON body. Two calls with the same function and args but different IDs are valid distinct calls. If there's no `call_id` in the Codex protocol, disable dedup entirely (emit all tool calls as-is).

  **7d. `codex_stream.rs` — use extracted usage metadata:**
  Lines 68-70 and 186-191: Instead of discarding `usageMetadata` with `let _ = ...`, store it and populate the `response.completed` event's `usage` fields with the extracted values (instead of hardcoded `0`s).

  **Risk level:** Low-Medium. Smart quote removal is safe (restoring original content). Tool call dedup change could surface duplicates if the model actually emits genuine duplicates, but suppressing valid calls is worse. Streaming gate requires careful testing of the non-streaming path.

  **Recommended Agent Profile:**
  - **Category**: `unspecified-low`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with WP4, WP5)
  - **Blocks**: None
  - **Blocked By**: None

  **Acceptance Criteria:**
  - [ ] Non-streaming path (`collect_to_json`) does NOT get graceful finish on errors — propagates errors for retry
  - [ ] Smart quote characters in generated content are preserved (no replacement)
  - [ ] Valid repeated tool calls are not suppressed by dedup
  - [ ] `response.completed` event contains actual usage metadata (not hardcoded 0s)
  - [ ] `cargo test --workspace` passes

  **Commit**: YES
  - Message: `fix(openai): gate graceful finish on streaming mode, preserve smart quotes, fix tool call dedup and usage`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 8. WP8: Miscellaneous Medium Issues

  **Issues addressed:**
  - `modules/proxy_db.rs` — **[Medium]** `save_log` receives `request_body: None` / `response_body: None` from monitor.rs (data loss)
  - `proxy/middleware/monitor.rs` — **[Medium]** The actual source of body=None (lines 117-118)
  - `antigravity-vps-cli/src/main.rs` — **[Medium]** CLI `command.join(" ")` breaks quoted/space-containing arguments
  - `deploy.sh` / `flake.nix` — **[High]** Nix closure deploy causes SIGBUS on VPS (requires investigation, not a code fix)
  - `proxy/handlers/` — **[Medium]** 10MB buffer per stream, still a concern at extreme concurrency (1000+ streams)
  - `token_manager/selection_helpers.rs` — **[Medium]** Thundering herd from pre-calculated load snapshot

  **Files to modify:**
  - `crates/antigravity-core/src/proxy/middleware/monitor.rs` (lines 117-118)
  - `antigravity-vps-cli/src/main.rs` (line 76)

  **Fix approach:**

  **8a. `monitor.rs` — populate request_body/response_body:**
  Lines 117-118: Instead of `request_body: None, response_body: None`, populate from the buffered data:
  - `request_body`: Use the already-buffered `body_bytes` (line 60) — `String::from_utf8(body_bytes.to_vec()).ok()`. Only if `content_length <= 512KB` (don't log huge bodies).
  - `response_body`: For JSON responses, the body is already parsed. For SSE, capture the last 8KB tail that's already being kept.
  This restores actual request/response logging to the SQLite proxy_db.

  **8b. `antigravity-vps-cli` — use shell-escaped arguments:**
  Line 76: Replace `command.join(" ")` with proper shell escaping. Use the `shell-escape` crate or manually wrap each argument: `command.iter().map(|a| shell_escape::escape(a.into())).collect::<Vec<_>>().join(" ")`. This preserves arguments with spaces, quotes, and special characters.

  **8c. `deploy.sh` SIGBUS — investigation task (not code fix):**
  This is a Nix closure linking issue, not a Rust code bug. The fix is likely in `flake.nix` build settings (static linking, different glibc, or different Nix channel). Mark as requires-investigation with documented workaround (scp cargo-built binary).

  **8d. Thundering herd (selection_helpers.rs) — document as accepted:**
  The thundering herd from stale load snapshot is already mitigated by `ActiveRequestGuard::try_new` which does a real-time check. The snapshot-based selection is "best effort" — the guard prevents actual concurrency violations. The remaining suboptimality (multiple threads picking same "best" candidate) is a performance concern, not a correctness bug. Fix would require lock-based reservation protocol which adds latency to every request. Document as accepted tradeoff.

  **8e. Stream buffer (proxy/handlers/) — monitor, don't fix:**
  10MB per stream is already mitigated from 50MB. Further reduction risks truncating legitimate large responses. Add Prometheus gauge metric `proxy_active_streams` to track concurrent stream count for capacity planning rather than further reducing the buffer.

  **Risk level:** Low. Monitor body logging is additive. CLI escaping is isolated to the VPS CLI crate.

  **Recommended Agent Profile:**
  - **Category**: `unspecified-low`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2/3 (with WP4-WP7)
  - **Blocks**: WP9
  - **Blocked By**: None

  **Acceptance Criteria:**
  - [ ] `proxy_db` receives actual request/response bodies (not None) for requests <= 512KB
  - [ ] CLI `command.join` replaced with proper shell escaping
  - [ ] Thundering herd documented as accepted tradeoff in AGENTS.md
  - [ ] `cargo test --workspace` passes

  **Commit**: YES
  - Message: `fix(misc): populate monitor log bodies, fix CLI argument escaping`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 9. WP9: AGENTS.md Accuracy Update

  **Issues addressed:**
  - 6 issues discovered to be already fixed during research (stale AGENTS.md entries)
  - Corrections to issue descriptions that misidentify the source file
  - Thundering herd documented as accepted tradeoff

  **Files to modify:**
  - `AGENTS.md`

  **Fix approach:**

  Mark as Fixed/Resolved:
  - `token_manager/selection.rs` — `get_token_forced` bypasses expiry → ALREADY CHECKS expiry + project ID (lines 82-89)
  - `token_manager/selection_helpers.rs` — O(N log N) DashMap lookups → ALREADY pre-fetches into Vec (lines 110-112, 206-210)
  - `token_manager/token_refresh.rs` — refresh_locks never removed → ALREADY cleaned every 60s (mod.rs line 158)
  - `proxy/mappers/tool_result_compressor/mod.rs` — regex recompilation → ALREADY uses OnceLock (lines 19-39)
  - `proxy/providers/zai_anthropic.rs` — DoS from info logging → logging ALREADY removed
  - `account_pg_targeted.rs` — overwrites all sessions → 1:1 PK constraint, no multi-session overwrite possible

  Correct source attribution:
  - `modules/proxy_db.rs` "save_log hardcodes to None" → correct to "monitor.rs lines 117-118 passes None to save_log"

  Add accepted tradeoff note:
  - `token_manager/selection_helpers.rs` thundering herd — mitigated by ActiveRequestGuard, accepted as performance tradeoff

  Update all issues fixed by WP1-WP8 (after they are completed).

  **Risk level:** None (documentation only).

  **Recommended Agent Profile:**
  - **Category**: `quick`
  - **Skills**: [`git-master`]

  **Parallelization:**
  - **Can Run In Parallel**: NO (final)
  - **Parallel Group**: Wave 3 (after all other WPs)
  - **Blocks**: None
  - **Blocked By**: ALL (WP1-WP8)

  **Acceptance Criteria:**
  - [ ] All 6 stale entries marked as Fixed with date and evidence
  - [ ] `proxy_db.rs` entry corrected to identify `monitor.rs` as the source
  - [ ] Thundering herd documented as accepted tradeoff
  - [ ] All WP1-WP8 fixes reflected in AGENTS.md

  **Commit**: YES
  - Message: `docs: update AGENTS.md Known Issues — mark resolved issues, correct attributions`
  - Pre-commit: `cargo clippy --workspace -- -Dwarnings`

---

## Commit Strategy

| After WP | Message | Verification |
|----------|---------|--------------|
| 1 | `fix(state): eliminate race conditions in config reload, account switching, and session eviction` | `cargo test --workspace` |
| 2 | `fix(io): wrap all blocking file/SQLite I/O in spawn_blocking, reduce monitor buffer to 512KB` | `cargo test --workspace` |
| 3 | `fix(claude): correct retry budget accounting, error classification, and stream error handling` | `cargo test --workspace` |
| 4 | `fix(server): persist routing state across hot-reloads, enable upstream circuit breaker` | `cargo test --workspace` |
| 5 | `fix(upstream): fail explicitly on proxy build errors instead of silent direct connection fallback` | `cargo test --workspace` |
| 6 | `fix(mappers): async file reads, schema depth limit, preserve client stop_sequences` | `cargo test --workspace` |
| 7 | `fix(openai): gate graceful finish on streaming mode, preserve smart quotes, fix tool call dedup and usage` | `cargo test --workspace` |
| 8 | `fix(misc): populate monitor log bodies, fix CLI argument escaping` | `cargo test --workspace` |
| 9 | `docs: update AGENTS.md Known Issues — mark resolved issues, correct attributions` | `cargo clippy --workspace -- -Dwarnings` |

---

## Success Criteria

### Verification Commands
```bash
cargo check --workspace           # Must pass
cargo clippy --workspace -- -Dwarnings  # Must pass (0 warnings)
cargo test --workspace            # Must pass (all 344+ tests)
cargo build --release -p antigravity-server  # Must build (11MB)
```

### Final Checklist
- [ ] All High severity issues addressed (4 issues)
- [ ] All Medium severity issues addressed (24 issues)
- [ ] 6 stale AGENTS.md entries corrected
- [ ] No breaking changes to API contract
- [ ] All existing tests pass
- [ ] No new clippy warnings