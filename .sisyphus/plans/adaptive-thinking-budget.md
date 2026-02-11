# Adaptive Thinking Budget — Port from Upstream

## TL;DR

> **Quick Summary**: Port the 4-mode thinking budget configuration (Auto, Passthrough, Custom, Adaptive) from upstream `lbjlaq/Antigravity-Manager` into our headless Axum fork. Replaces compile-time constants with runtime-configurable `ThinkingBudgetConfig` persisted in `gui_config.json`.
> 
> **Deliverables**:
> - `ThinkingBudgetMode` enum + `ThinkingBudgetConfig` struct in `antigravity-types`
> - Global static config accessor (`get_thinking_budget_config()` / `update_thinking_budget_config()`)
> - Mode-aware generation config in Claude + OpenAI paths
> - Adaptive mode in Gemini wrapper (`thinkingLevel` / `thinkingBudget:-1` / `maxOutputTokens:131072`)
> - Hot-reload wiring + startup init
> - ~20 TDD tests
> 
> **Estimated Effort**: Medium
> **Parallel Execution**: YES — 2 waves
> **Critical Path**: Task 1 → Tasks 2,3,4 (parallel) → Task 5 → Task 6

---

## Context

### Original Request

Port the "Adaptive thinking budget" feature from upstream `lbjlaq/Antigravity-Manager`. Upstream has `ThinkingBudgetMode` enum (Auto/Passthrough/Custom/Adaptive) with `ThinkingBudgetConfig` struct. Our fork has 3 hardcoded constants. Need runtime config across all 3 request paths.

### Interview Summary

**Key Discussions**:
- User specified exact struct shape and behavior for each mode
- User confirmed TDD approach, types in `antigravity-types`, logic in `antigravity-core`
- User confirmed default should be Adaptive mode (NOTE: Metis flagged backward compat — see Guardrails)

**Research Findings**:
- Current state: `THINKING_BUDGET=16000`, `THINKING_OVERHEAD=32768`, `THINKING_MIN_OVERHEAD=8192` in `thinking_constants.rs`
- Two AppState structs: server-level (`antigravity-server/src/state/mod.rs`) and proxy-level (`proxy/server.rs`)
- Generation config builders are pure functions — no state access
- Upstream uses global static `OnceLock<RwLock<ThinkingBudgetConfig>>` — matches existing `SignatureCache::global()` precedent in our codebase
- Upstream Adaptive mode: Gemini 3 → `thinkingLevel`, Gemini 2.x → `thinkingBudget: -1`, both → `maxOutputTokens: 131072`
- Upstream Adaptive is PARTIAL upstream (OpenAI path = passthrough stub, Claude path has no Adaptive arm in match — real logic in `wrapper.rs`)
- Flash cap (24576) in `wrapper.rs` runs AFTER generation config → Flash cap always wins over Custom
- Effort mapping already exists in our Claude generation config (lines 76-90) — orthogonal, do not touch

### Metis Review

**Identified Gaps** (addressed):
- **Backward compatibility**: `#[serde(default)]` must produce Auto mode matching current constants (16000 + 32768 = 48768). Tests added.
- **Dual modification site**: Generation config builds thinkingConfig → wrapper.rs may override for Adaptive. Plan explicitly sequences: gen config first, wrapper second.
- **`thinkingBudget: -1` type safety**: Our `THINKING_BUDGET` is `u64`, sentinel `-1` requires `i64`. JSON handles transparently but Rust comparisons must use `i64`.
- **Custom(30000) + Flash**: Flash cap always wins — must preserve ordering in wrapper.rs.
- **`maxOutputTokens: 131072` on Flash**: May exceed Flash output limit. Metis flagged as assumption requiring empirical validation.
- **Passthrough with no client budget**: Must inject default (current THINKING_BUDGET) — same as current Auto behavior.
- **Adaptive + non-thinking model**: Generation config already skips thinkingConfig for non-thinking models (`is_thinking_enabled=false`). Adaptive mode only activates when thinking is already enabled.
- **`thinking.type = "adaptive"` upstream bug**: Upstream's `is_thinking_enabled` only checks `type_ == "enabled"`. Do NOT port this bug — out of scope (separate task to add `"adaptive"` type support).

---

## Work Objectives

### Core Objective
Replace compile-time thinking budget constants with a runtime-configurable `ThinkingBudgetConfig` supporting 4 modes, persisted in `gui_config.json`, hot-reloadable, defaulting to Adaptive mode.

### Concrete Deliverables
- `crates/antigravity-types/src/models/config/thinking.rs` — new file with enum + struct
- `crates/antigravity-types/src/models/config/proxy.rs` — `thinking_budget` field on ProxyConfig
- `crates/antigravity-core/src/proxy/common/thinking_config.rs` — new file with global static + accessors
- `crates/antigravity-core/src/proxy/mappers/claude/request/generation_config.rs` — mode-aware logic
- `crates/antigravity-core/src/proxy/mappers/openai/request/generation_config.rs` — mode-aware logic
- `crates/antigravity-core/src/proxy/mappers/gemini/wrapper.rs` — Adaptive detection + thinkingLevel/-1
- `antigravity-server/src/state/accessors.rs` — hot-reload wiring
- `antigravity-server/src/main.rs` — startup init
- Test files covering all modes × paths

### Definition of Done
- [ ] `cargo check --workspace` passes
- [ ] `cargo clippy --workspace -- -Dwarnings` passes (zero warnings)
- [ ] `cargo test -p antigravity-types` passes (existing + new tests)
- [ ] `cargo test -p antigravity-core --lib` passes (170+ existing + new tests)
- [ ] Deserializing existing `gui_config.json` (no `thinking_budget` field) produces backward-compatible behavior
- [ ] All 4 modes produce correct thinkingConfig JSON for each path

### Must Have
- Backward-compatible defaults: Auto mode must produce same output as current constants
- All 4 modes: Auto, Passthrough, Custom, Adaptive
- Adaptive mode: `thinkingLevel` for Gemini 3, `thinkingBudget: -1` for Gemini 2, `maxOutputTokens: 131072`
- Hot-reload from `gui_config.json`
- Flash cap preserved (24576 hard limit on Flash, regardless of mode)
- Serde round-trip: JSON → struct → JSON preserves all fields

### Must NOT Have (Guardrails)
- ❌ API endpoint for runtime mode changes (hot reload from config file = sufficient)
- ❌ Per-model thinking budget configs (one global config)
- ❌ `thinking.type = "adaptive"` support in Claude request parsing (separate task)
- ❌ Changes to effort level mapping (lines 76-90 of Claude generation_config.rs — orthogonal feature)
- ❌ Changes to Flash cap logic in `wrapper.rs` beyond adding Adaptive detection BEFORE it
- ❌ New `Arc<RwLock<>>` field on proxy-level AppState — global static IS the config store
- ❌ Database persistence (lives in `gui_config.json` via ProxyConfig)
- ❌ Leptos/UI changes (headless server)
- ❌ Function signature changes to `build_generation_config` in either path
- ❌ Removal of `thinking_constants.rs` (keep as constants for non-mode-aware code)

---

## Verification Strategy

> **UNIVERSAL RULE: ZERO HUMAN INTERVENTION**
>
> ALL tasks verified by agent using commands and tool calls.

### Test Decision
- **Infrastructure exists**: YES (`cargo test`)
- **Automated tests**: YES (TDD — RED-GREEN-REFACTOR)
- **Framework**: `cargo test` (standard Rust test framework)

### Precedence Chain (CRITICAL — document for all implementors)

```
Client budget_tokens → ThinkingBudgetMode override → Flash cap (24576) → Adaptive override
```

| Mode | Client sends budget | Server config | Final thinkingBudget | maxOutputTokens |
|------|-------------------|---------------|---------------------|-----------------|
| Auto | 5000 | — | 5000 (passthrough) | max(5000, budget + 32768) |
| Auto | None | — | 16000 (THINKING_BUDGET) | 16000 + 32768 = 48768 |
| Auto | 30000, Flash model | — | 24576 (Flash cap) | max(30000, 24576 + 32768) |
| Passthrough | 5000 | — | 5000 | max(5000, 5000 + 32768) |
| Passthrough | None | — | 16000 (fallback) | 16000 + 32768 |
| Custom | Any | custom_value=30000 | 30000 | max(client, 30000 + 32768) |
| Custom | Any, Flash | custom_value=30000 | 24576 (Flash cap wins) | max(client, 24576 + 32768) |
| Adaptive | Any, Gemini 3 | effort=high | thinkingLevel: "HIGH" | 131072 |
| Adaptive | Any, Gemini 3 | effort=low | thinkingLevel: "LOW" | 131072 |
| Adaptive | Any, Gemini 2 | — | thinkingBudget: -1 | 131072 |

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately):
└── Task 1: Types + global static + serde tests [no dependencies]

Wave 2 (After Wave 1):
├── Task 2: Claude generation config mode logic [depends: 1]
├── Task 3: OpenAI generation config mode logic [depends: 1]
└── Task 4: Gemini wrapper Adaptive mode [depends: 1]

Wave 3 (After Wave 2):
└── Task 5: Hot-reload + startup wiring [depends: 1, 2, 3, 4]

Wave 4 (After Wave 3):
└── Task 6: Integration tests + backward compat verification [depends: 5]

Critical Path: Task 1 → Task 2 → Task 5 → Task 6
Parallel Speedup: ~30% (Wave 2 runs 3 tasks in parallel)
```

### Dependency Matrix

| Task | Depends On | Blocks | Can Parallelize With |
|------|------------|--------|---------------------|
| 1 | None | 2, 3, 4 | None (foundational) |
| 2 | 1 | 5 | 3, 4 |
| 3 | 1 | 5 | 2, 4 |
| 4 | 1 | 5 | 2, 3 |
| 5 | 1, 2, 3, 4 | 6 | None |
| 6 | 5 | None | None (final) |

### Agent Dispatch Summary

| Wave | Tasks | Recommended Agents |
|------|-------|-------------------|
| 1 | 1 | `delegate_task(category="unspecified-high", load_skills=[], run_in_background=false)` |
| 2 | 2, 3, 4 | `delegate_task(category="quick", ...)` × 3 in parallel |
| 3 | 5 | `delegate_task(category="quick", ...)` |
| 4 | 6 | `delegate_task(category="unspecified-low", ...)` |

---

## TODOs

- [ ] 1. Types, Global Static, and Serde Tests

  **What to do**:

  **1a. Create `thinking.rs` in antigravity-types config module:**
  ```rust
  // crates/antigravity-types/src/models/config/thinking.rs
  
  #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
  #[serde(rename_all = "snake_case")]
  pub enum ThinkingBudgetMode {
      Auto,
      Passthrough,
      Custom,
      Adaptive,
  }
  
  impl Default for ThinkingBudgetMode {
      fn default() -> Self { Self::Adaptive }
  }
  
  #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
  pub struct ThinkingBudgetConfig {
      #[serde(default)]
      pub mode: ThinkingBudgetMode,
      #[serde(default = "default_thinking_budget_custom_value")]
      pub custom_value: u32,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub effort: Option<String>,
  }
  
  fn default_thinking_budget_custom_value() -> u32 { 24576 }
  
  impl Default for ThinkingBudgetConfig {
      fn default() -> Self {
          Self {
              mode: ThinkingBudgetMode::Adaptive,
              custom_value: 24576,
              effort: None,
          }
      }
  }
  ```

  **1b. Register in config module:**
  - Add `mod thinking;` to `crates/antigravity-types/src/models/config/mod.rs`
  - Add `pub use thinking::{ThinkingBudgetConfig, ThinkingBudgetMode};` to the re-exports

  **1c. Add `thinking_budget` field to ProxyConfig:**
  ```rust
  // In proxy.rs, add field:
  #[serde(default)]
  pub thinking_budget: ThinkingBudgetConfig,
  ```
  Also add `thinking_budget: ThinkingBudgetConfig::default()` to `Default for ProxyConfig`.

  **1d. Create global static in antigravity-core:**
  ```rust
  // crates/antigravity-core/src/proxy/common/thinking_config.rs
  
  use antigravity_types::models::ThinkingBudgetConfig;
  use std::sync::{OnceLock, RwLock};
  
  static GLOBAL_THINKING_BUDGET: OnceLock<RwLock<ThinkingBudgetConfig>> = OnceLock::new();
  
  pub fn get_thinking_budget_config() -> ThinkingBudgetConfig {
      GLOBAL_THINKING_BUDGET
          .get()
          .and_then(|lock| lock.read().ok())
          .map(|cfg| cfg.clone())
          .unwrap_or_default()
  }
  
  pub fn update_thinking_budget_config(config: ThinkingBudgetConfig) {
      let lock = GLOBAL_THINKING_BUDGET.get_or_init(|| RwLock::new(ThinkingBudgetConfig::default()));
      if let Ok(mut guard) = lock.write() {
          *guard = config;
      }
  }
  ```
  Register `mod thinking_config;` in `crates/antigravity-core/src/proxy/common/mod.rs` and `pub use thinking_config::*;` appropriately.

  **1e. Write TDD tests (RED first):**
  - Inline `#[cfg(test)] mod tests` in `thinking.rs`:
    - `test_default_mode_is_adaptive` — `ThinkingBudgetConfig::default().mode == Adaptive`
    - `test_default_custom_value` — `ThinkingBudgetConfig::default().custom_value == 24576`
    - `test_serde_round_trip_all_modes` — serialize then deserialize each mode variant
    - `test_serde_missing_thinking_budget_field` — deserialize ProxyConfig JSON without `thinking_budget` → gets default
    - `test_serde_explicit_auto_mode` — `{"mode": "auto"}` → Auto
    - `test_effort_serialization` — effort=Some("high") serializes, None skips
  - Inline `#[cfg(test)] mod tests` in `thinking_config.rs`:
    - `test_get_before_init_returns_default` — calling `get_thinking_budget_config()` before any `update` returns default
    - `test_update_and_get` — update with Custom(30000) → get returns Custom(30000)

  **Must NOT do**:
  - Do NOT add validation logic to types crate (validation happens at usage site)
  - Do NOT add methods beyond Default impl to ThinkingBudgetConfig
  - Do NOT modify any existing generation config code in this task

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Foundation task creating types + global static across two crates — moderate complexity, must be correct
  - **Skills**: `[]`
    - No specialized skills needed — pure Rust type definitions

  **Parallelization**:
  - **Can Run In Parallel**: NO (foundational)
  - **Parallel Group**: Wave 1 (solo)
  - **Blocks**: Tasks 2, 3, 4, 5, 6
  - **Blocked By**: None

  **References**:

  **Pattern References**:
  - `crates/antigravity-types/src/models/config/enums.rs:7-19` — Enum serde pattern with `#[serde(rename_all = "snake_case")]` and `#[derive(Default)]`
  - `crates/antigravity-types/src/models/config/session.rs` — Config struct pattern with `#[serde(default)]` fields, `impl Default`, and `Validate` derive
  - `crates/antigravity-types/src/models/config/mod.rs` — Module registration and re-export pattern
  - `crates/antigravity-types/src/models/config/proxy.rs:12-85` — ProxyConfig struct: how to add a new `#[serde(default)]` field with Default impl
  - `crates/antigravity-core/src/proxy/common/thinking_constants.rs` — Current constants (16000, 32768, 8192) — Auto mode default MUST match these

  **Architecture References**:
  - `crates/antigravity-core/src/proxy/signature_cache/mod.rs` — `SignatureCache::global()` pattern with `OnceLock` — follow this exact pattern for the global static
  - `crates/antigravity-core/src/proxy/common/mod.rs` — Where to register `thinking_config` module

  **Test References**:
  - `crates/antigravity-types/src/models/quota.rs` — Inline `#[cfg(test)] mod tests` pattern for serde/default testing in types crate
  - `crates/antigravity-types/src/models/token.rs` — Test pattern for types crate

  **WHY Each Reference Matters**:
  - `enums.rs` — The executor must follow the exact derive+serde pattern for `ThinkingBudgetMode` to match existing config enums
  - `proxy.rs` — The executor must see how to add a new defaulted field without breaking existing deserialization
  - `signature_cache/mod.rs` — The global static pattern is proven in the codebase; executor must replicate, not invent
  - `thinking_constants.rs` — The executor must know what values Auto mode must reproduce for backward compat

  **Acceptance Criteria**:

  - [ ] `crates/antigravity-types/src/models/config/thinking.rs` exists, <100 lines
  - [ ] `ThinkingBudgetMode` has 4 variants: Auto, Passthrough, Custom, Adaptive
  - [ ] `ThinkingBudgetConfig::default()` returns `{mode: Adaptive, custom_value: 24576, effort: None}`
  - [ ] `thinking_budget` field exists on `ProxyConfig` with `#[serde(default)]`
  - [ ] Deserializing `{"enabled":true,"port":8045,"api_key":"test","auto_start":true}` (no thinking_budget) produces `ThinkingBudgetConfig::default()` in ProxyConfig
  - [ ] `crates/antigravity-core/src/proxy/common/thinking_config.rs` exists, <60 lines
  - [ ] `get_thinking_budget_config()` returns default before any `update_thinking_budget_config()` call
  - [ ] `update_thinking_budget_config(config)` + `get_thinking_budget_config()` returns updated config
  - [ ] `cargo test -p antigravity-types` passes (all existing + 6 new tests)
  - [ ] `cargo test -p antigravity-core --lib` passes (170+ existing + 2 new tests)
  - [ ] `cargo clippy --workspace -- -Dwarnings` passes

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: All tests pass after type definitions
    Tool: Bash (cargo)
    Preconditions: Task 1 code written
    Steps:
      1. cargo test -p antigravity-types 2>&1
      2. Assert: exit code 0, "test result: ok" in output
      3. cargo test -p antigravity-core --lib 2>&1
      4. Assert: exit code 0, "test result: ok" in output, ≥172 tests (170 existing + 2 new)
      5. cargo clippy --workspace -- -Dwarnings 2>&1
      6. Assert: exit code 0, no warnings
    Expected Result: All tests pass, zero clippy warnings
    Evidence: Terminal output captured

  Scenario: Serde backward compatibility
    Tool: Bash (cargo test specific test)
    Preconditions: Task 1 tests written
    Steps:
      1. cargo test -p antigravity-types test_serde_missing_thinking_budget_field -- --nocapture
      2. Assert: exit code 0, test passes
    Expected Result: Deserializing old config JSON without thinking_budget field produces valid defaults
    Evidence: Test output captured
  ```

  **Commit**: YES
  - Message: `feat(types): add ThinkingBudgetMode enum and ThinkingBudgetConfig struct`
  - Files: `thinking.rs`, `mod.rs`, `proxy.rs`, `thinking_config.rs`, `common/mod.rs`
  - Pre-commit: `cargo test -p antigravity-types && cargo test -p antigravity-core --lib && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 2. Claude Generation Config — Mode-Aware Logic

  **What to do**:

  Modify `crates/antigravity-core/src/proxy/mappers/claude/request/generation_config.rs` to read `ThinkingBudgetConfig` from global static and apply mode logic.

  **Current behavior (to preserve as Auto mode)**:
  - Client has `thinking.budget_tokens` → use it (cap to 24576 for Flash)
  - Client has no thinking → inject `THINKING_BUDGET` (16000)
  - maxOutputTokens: `budget + THINKING_OVERHEAD` when client's max_tokens ≤ budget

  **Changes**:
  1. Add import: `use crate::proxy::common::thinking_config::get_thinking_budget_config;`
  2. Add import: `use antigravity_types::models::ThinkingBudgetMode;`
  3. After determining `budget` (line ~50), apply mode override:

  ```rust
  let tb_config = get_thinking_budget_config();
  let budget = match tb_config.mode {
      ThinkingBudgetMode::Auto => budget, // current behavior unchanged
      ThinkingBudgetMode::Passthrough => budget, // use client's value as-is
      ThinkingBudgetMode::Custom => tb_config.custom_value,
      ThinkingBudgetMode::Adaptive => budget, // generation config passthrough; actual adaptive logic in wrapper.rs
  };
  ```

  4. For Adaptive mode, also inject effort-based `thinkingLevel` when the mapped model is Gemini 3:
     - Extract effort from `claude_req.output_config.effort` OR `claude_req.thinking.effort`
     - Map: low→"LOW", medium→"MEDIUM", high/max→"HIGH", default→"HIGH"
     - Set `thinking_config["thinkingLevel"] = json!(level)` INSTEAD of `thinking_config["thinkingBudget"]`
     - For non-Gemini-3 models in Adaptive: set `thinking_config["thinkingBudget"] = json!(-1_i64)`

  5. For Adaptive mode, set `maxOutputTokens = 131072` (instead of budget + overhead).

  **CRITICAL**: The function does NOT know the mapped model name. It receives `claude_req` which has the CLIENT model name (e.g., "claude-opus-4-5"). The mapped model is resolved in `monolith.rs`. Two options:
     - (A) Add `mapped_model: &str` parameter to `build_generation_config` → breaks function signature (FORBIDDEN by guardrails)
     - (B) Use `claude_req.model` to detect: if model name contains "gemini-3" → thinkingLevel; else → thinkingBudget:-1. But Claude client models are "claude-*", not "gemini-*" — the mapping happens externally.
     - **(C) RECOMMENDED**: In Adaptive mode, generation config sets `thinkingBudget: -1` unconditionally. The Gemini wrapper (Task 4) handles the Gemini 3 → thinkingLevel conversion because wrapper.rs HAS the mapped model name. This matches upstream's actual architecture.

  6. Keep `THINKING_BUDGET` and `THINKING_OVERHEAD` imports for Auto/Passthrough fallbacks.

  **Must NOT do**:
  - Do NOT change function signature of `build_generation_config`
  - Do NOT modify effort level mapping (lines 76-90) — it's orthogonal and already works
  - Do NOT touch stopSequences, temperature, topP, topK handling
  - Do NOT remove existing Flash cap logic (lines 34-38)

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Single file modification with clear before/after — mode branching around existing logic
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 3, 4)
  - **Blocks**: Task 5
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `crates/antigravity-core/src/proxy/mappers/claude/request/generation_config.rs` (FULL FILE) — This is the file being modified. Read ALL 122 lines before any changes.
  - `crates/antigravity-core/src/proxy/mappers/claude/request/monolith.rs:190-191` — Call site: `build_generation_config(claude_req, has_web_search_tool, is_thinking_enabled)` — DO NOT CHANGE this call

  **API/Type References**:
  - `crates/antigravity-types/src/models/config/thinking.rs` — `ThinkingBudgetMode` enum (created in Task 1)
  - `crates/antigravity-core/src/proxy/common/thinking_config.rs` — `get_thinking_budget_config()` (created in Task 1)
  - `crates/antigravity-core/src/proxy/common/thinking_constants.rs` — `THINKING_BUDGET`, `THINKING_OVERHEAD` — keep importing for Auto/Passthrough

  **WHY Each Reference Matters**:
  - `generation_config.rs` — MUST read the full file to understand existing thinking budget injection at lines 27-61 and maxOutputTokens at lines 96-115
  - `monolith.rs:190` — Confirms function signature is `(&ClaudeRequest, bool, bool)` — cannot add parameters
  - `thinking.rs` — Need to import `ThinkingBudgetMode` for the match expression

  **Acceptance Criteria**:

  - [ ] `build_generation_config` reads `get_thinking_budget_config()` to determine mode
  - [ ] Auto mode: identical JSON output to current behavior (regression test)
  - [ ] Passthrough mode: client's `budget_tokens` passed through unchanged
  - [ ] Custom mode: `custom_value` overrides client budget
  - [ ] Adaptive mode: `thinkingBudget: -1` set (wrapper.rs handles thinkingLevel conversion)
  - [ ] Adaptive mode: `maxOutputTokens: 131072` set
  - [ ] File stays under 300 lines
  - [ ] `cargo test -p antigravity-core --lib` passes

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Auto mode backward compatibility
    Tool: Bash (cargo test)
    Preconditions: Task 1 + Task 2 code written
    Steps:
      1. Write test: set global config to Auto mode
      2. Call build_generation_config with no client thinking → assert thinkingBudget == 16000
      3. Call build_generation_config with max_tokens=5000 → assert maxOutputTokens == 16000 + 32768
      4. cargo test -p antigravity-core --lib test_claude_gen_config_auto -- --nocapture
      5. Assert: exit code 0
    Expected Result: Auto mode matches current behavior exactly
    Evidence: Test output captured

  Scenario: Adaptive mode sets budget -1
    Tool: Bash (cargo test)
    Steps:
      1. Write test: set global config to Adaptive mode
      2. Call build_generation_config with thinking enabled
      3. Assert thinkingConfig.thinkingBudget == -1
      4. Assert maxOutputTokens == 131072
    Expected Result: Adaptive injects sentinel -1 and 131072
    Evidence: Test output captured
  ```

  **Commit**: YES (groups with Task 3)
  - Message: `feat(proxy): mode-aware thinking budget in Claude and OpenAI generation config`
  - Files: `claude/request/generation_config.rs`, `openai/request/generation_config.rs`
  - Pre-commit: `cargo test -p antigravity-core --lib && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 3. OpenAI Generation Config — Mode-Aware Logic

  **What to do**:

  Modify `crates/antigravity-core/src/proxy/mappers/openai/request/generation_config.rs` to read `ThinkingBudgetConfig` and apply mode logic. Symmetric to Task 2 but in OpenAI path.

  **Current behavior (to preserve as Auto mode)**:
  - Thinking enabled → `thinkingBudget: THINKING_BUDGET` (16000), `includeThoughts: true`
  - Client `max_tokens ≤ 16000` → maxOutputTokens = `THINKING_BUDGET + THINKING_MIN_OVERHEAD` (24192)
  - No client `max_tokens` → maxOutputTokens = `THINKING_BUDGET + THINKING_OVERHEAD` (48768)
  - Client `max_tokens > 16000` → maxOutputTokens = client value

  **Changes**:
  1. Add imports for `get_thinking_budget_config`, `ThinkingBudgetMode`
  2. Inside `if actual_include_thinking` block (line 40), apply mode:

  ```rust
  let tb_config = get_thinking_budget_config();
  let budget = match tb_config.mode {
      ThinkingBudgetMode::Auto => THINKING_BUDGET,
      ThinkingBudgetMode::Passthrough => THINKING_BUDGET, // OpenAI path has no client thinking budget — fallback
      ThinkingBudgetMode::Custom => tb_config.custom_value as u64,
      ThinkingBudgetMode::Adaptive => {
          // Set thinkingBudget: -1 for adaptive (wrapper.rs handles thinkingLevel)
          // maxOutputTokens handled below
          0 // sentinel — handled separately
      }
  };
  ```

  3. For Adaptive mode:
     - Set `thinkingConfig["thinkingBudget"] = json!(-1_i64)` (instead of budget)
     - Set `maxOutputTokens = 131072`

  4. For non-Adaptive modes, keep existing maxOutputTokens calculation logic.

  **NOTE**: OpenAI path has `actual_include_thinking` which is already resolved by the handler. The generation config builder just needs to vary the budget value.

  **Must NOT do**:
  - Do NOT change function signature `build_generation_config(&OpenAIRequest, bool, &str)`
  - Do NOT modify topP sanitization, stop sequences, response format handling
  - Do NOT change non-thinking path (lines 67-72)

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Single file, symmetric to Task 2, clear pattern to follow
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 2, 4)
  - **Blocks**: Task 5
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `crates/antigravity-core/src/proxy/mappers/openai/request/generation_config.rs` (FULL FILE) — This is the file being modified. Read ALL 90 lines.
  - `crates/antigravity-core/src/proxy/mappers/openai/request/mod.rs:178` — Call site: `build_generation_config(request, actual_include_thinking, mapped_model)` — DO NOT CHANGE this call
  - Task 2 implementation — Follow same mode branching pattern for consistency

  **API/Type References**:
  - `crates/antigravity-types/src/models/config/thinking.rs` — `ThinkingBudgetMode` (created in Task 1)
  - `crates/antigravity-core/src/proxy/common/thinking_config.rs` — `get_thinking_budget_config()` (created in Task 1)
  - `crates/antigravity-core/src/proxy/common/thinking_constants.rs` — All 3 constants still needed for Auto mode

  **WHY Each Reference Matters**:
  - `generation_config.rs` — OpenAI path has DIFFERENT maxOutputTokens logic than Claude path (uses THINKING_MIN_OVERHEAD for small client max_tokens). Must understand before modifying.
  - `mod.rs:178` — Confirms the builder has `mapped_model` as parameter (unlike Claude path) — could potentially use it for Gemini 3 detection, but not needed since wrapper.rs handles it

  **Acceptance Criteria**:

  - [ ] `build_generation_config` reads `get_thinking_budget_config()` to determine mode
  - [ ] Auto mode: identical output to current behavior (regression test)
  - [ ] Custom mode: `custom_value` used as thinkingBudget
  - [ ] Adaptive mode: `thinkingBudget: -1`, `maxOutputTokens: 131072`
  - [ ] File stays under 300 lines
  - [ ] `cargo test -p antigravity-core --lib` passes

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: OpenAI Auto mode regression
    Tool: Bash (cargo test)
    Steps:
      1. Set global config to Auto
      2. Call build_generation_config with no max_tokens, thinking=true
      3. Assert thinkingBudget == 16000, maxOutputTokens == 48768
      4. cargo test specific test
    Expected Result: Matches current hardcoded behavior
    Evidence: Test output

  Scenario: OpenAI Adaptive mode
    Tool: Bash (cargo test)
    Steps:
      1. Set global config to Adaptive
      2. Call build_generation_config with thinking=true
      3. Assert thinkingBudget == -1, maxOutputTokens == 131072
    Expected Result: Adaptive sentinel values injected
    Evidence: Test output
  ```

  **Commit**: YES (groups with Task 2 — same commit)
  - Message: (shared with Task 2)
  - Pre-commit: `cargo test -p antigravity-core --lib && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 4. Gemini Wrapper — Adaptive Detection + thinkingLevel

  **What to do**:

  Modify `crates/antigravity-core/src/proxy/mappers/gemini/wrapper.rs` to detect Adaptive mode and convert `thinkingBudget: -1` into `thinkingLevel` for Gemini 3 models.

  **Current wrapper.rs behavior**:
  - Line 51-68: Flash model cap — caps `thinkingBudget` to 24576 for Flash models

  **Changes**:
  1. Add import for `get_thinking_budget_config`, `ThinkingBudgetMode`
  2. Add Adaptive mode detection BEFORE Flash cap (between line 47 and line 49):

  ```rust
  // Adaptive thinking mode handling
  let tb_config = get_thinking_budget_config();
  if matches!(tb_config.mode, ThinkingBudgetMode::Adaptive) {
      if let Some(gen_config) = inner_request.get_mut("generationConfig") {
          if let Some(thinking_config) = gen_config.get_mut("thinkingConfig") {
              let is_gemini_3 = final_model_name.to_lowercase().contains("gemini-3");
              if is_gemini_3 {
                  // Gemini 3: use thinkingLevel instead of thinkingBudget
                  let effort = tb_config.effort.as_deref().unwrap_or("high");
                  let level = match effort.to_lowercase().as_str() {
                      "low" => "LOW",
                      "medium" => "MEDIUM",
                      "high" | "max" => "HIGH",
                      _ => "HIGH",
                  };
                  thinking_config["thinkingLevel"] = json!(level);
                  // Remove thinkingBudget — thinkingLevel and thinkingBudget are mutually exclusive
                  if let Some(obj) = thinking_config.as_object_mut() {
                      obj.remove("thinkingBudget");
                  }
              }
              // For non-Gemini-3: thinkingBudget: -1 passes through (set by gen config)
              
              // Adaptive always uses large maxOutputTokens
              gen_config["maxOutputTokens"] = json!(131072_i64);
          }
      }
  }
  ```

  3. Ensure Flash cap (lines 51-68) still runs AFTER Adaptive handling but skips when `thinkingLevel` is set (no thinkingBudget to cap):

  ```rust
  // Flash cap only applies when thinkingBudget exists (not thinkingLevel)
  if final_model_name.to_lowercase().contains("flash") {
      if let Some(gen_config) = inner_request.get_mut("generationConfig") {
          if let Some(thinking_config) = gen_config.get_mut("thinkingConfig") {
              // Skip cap if thinkingLevel is set (Adaptive mode on Gemini 3)
              if thinking_config.get("thinkingLevel").is_none() {
                  if let Some(budget_val) = thinking_config.get("thinkingBudget") {
                      // existing cap logic...
                  }
              }
          }
      }
  }
  ```

  **Must NOT do**:
  - Do NOT remove existing Flash cap logic — only add skip condition for thinkingLevel
  - Do NOT change signature, system instruction injection, or tool cleaning
  - Do NOT modify `unwrap_response`
  - Do NOT move Flash cap to a different location — keep it after Adaptive handling

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Single file, localized change — add adaptive block before existing Flash cap
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with Tasks 2, 3)
  - **Blocks**: Task 5
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `crates/antigravity-core/src/proxy/mappers/gemini/wrapper.rs` (FULL FILE) — This is the file being modified. Read ALL 221 lines. Focus on lines 49-68 (Flash cap) and understand the full request flow.
  - `crates/antigravity-core/src/proxy/mappers/gemini/wrapper_tests.rs` — Existing wrapper tests — follow their patterns

  **API/Type References**:
  - `crates/antigravity-types/src/models/config/thinking.rs` — `ThinkingBudgetMode` (Task 1)
  - `crates/antigravity-core/src/proxy/common/thinking_config.rs` — `get_thinking_budget_config()` (Task 1)

  **External References**:
  - Upstream `wrapper.rs` L120-232 (lbjlaq/Antigravity-Manager) — Reference for Adaptive detection logic: checks for `thinkingLevel` presence OR `thinkingBudget == -1` to determine adaptive state

  **WHY Each Reference Matters**:
  - `wrapper.rs` — Flash cap location (lines 51-68) is CRITICAL — Adaptive block must go BEFORE it, not after
  - `wrapper_tests.rs` — Must add tests following existing patterns (construct JSON body, call `wrap_request`, assert output)

  **Acceptance Criteria**:

  - [ ] Adaptive mode + Gemini 3: `thinkingLevel` field set (HIGH/MEDIUM/LOW), no `thinkingBudget` field
  - [ ] Adaptive mode + Gemini 2: `thinkingBudget: -1` preserved, `maxOutputTokens: 131072`
  - [ ] Adaptive mode + Flash + Gemini 3: `thinkingLevel` set, Flash cap NOT applied (no thinkingBudget to cap)
  - [ ] Non-Adaptive + Flash: existing Flash cap still works (24576 limit)
  - [ ] File stays under 300 lines
  - [ ] `cargo test -p antigravity-core --lib` passes (existing wrapper tests + new)

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Adaptive Gemini 3 sets thinkingLevel
    Tool: Bash (cargo test)
    Steps:
      1. Set global config to Adaptive, effort=None (default HIGH)
      2. Construct body with thinkingConfig containing thinkingBudget: -1
      3. Call wrap_request with mapped_model="gemini-3-pro"
      4. Assert: thinkingConfig has thinkingLevel: "HIGH", no thinkingBudget
      5. Assert: maxOutputTokens == 131072
    Expected Result: Gemini 3 uses thinkingLevel instead of thinkingBudget
    Evidence: Test output

  Scenario: Non-Adaptive Flash cap still works
    Tool: Bash (cargo test)
    Steps:
      1. Set global config to Auto
      2. Construct body with thinkingBudget: 30000
      3. Call wrap_request with mapped_model="gemini-3-flash"
      4. Assert: thinkingBudget == 24576 (capped)
    Expected Result: Flash cap preserved for non-Adaptive modes
    Evidence: Test output
  ```

  **Commit**: YES
  - Message: `feat(proxy): adaptive thinking mode in Gemini wrapper (thinkingLevel + budget -1)`
  - Files: `wrapper.rs`, `wrapper_tests.rs`
  - Pre-commit: `cargo test -p antigravity-core --lib && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 5. Hot-Reload Wiring + Startup Init

  **What to do**:

  Wire `update_thinking_budget_config()` into two places:
  1. Server startup (main.rs) — initialize global static from ProxyConfig
  2. Hot reload (accessors.rs) — update global static when config changes

  **5a. Startup wiring in `antigravity-server/src/main.rs`:**
  - After `let proxy_config = app_config.proxy;` (or wherever ProxyConfig is available)
  - Add: `antigravity_core::proxy::common::thinking_config::update_thinking_budget_config(proxy_config.thinking_budget.clone());`
  - Must be BEFORE `AppState::new_with_components()` call

  Find the exact location by reading `main.rs` — look for where `proxy_config` is first used.

  **5b. Hot reload wiring in `antigravity-server/src/state/accessors.rs`:**
  - In `hot_reload_proxy_config()` (line 105), after `let proxy_config = app_config.proxy;` (line 120):
  - Add: `antigravity_core::proxy::common::thinking_config::update_thinking_budget_config(proxy_config.thinking_budget.clone());`
  - BEFORE the write guards are acquired (line 124) — global static update is independent of AppState locks

  **Must NOT do**:
  - Do NOT add `thinking_budget` to proxy-level `AppState` struct in `server.rs`
  - Do NOT add `Arc<RwLock<>>` field — global static is the config store
  - Do NOT change `build_proxy_router_with_shared_state` signature

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Two small insertions (one line each) in existing functions
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 3 (solo)
  - **Blocks**: Task 6
  - **Blocked By**: Tasks 1, 2, 3, 4

  **References**:

  **Pattern References**:
  - `antigravity-server/src/state/accessors.rs:105-139` — `hot_reload_proxy_config()` — the EXACT function to modify. Note line 120 (`let proxy_config = app_config.proxy;`) — add the update call right after this line, before line 124.
  - `antigravity-server/src/main.rs` — Find where `ProxyConfig` is first available and used to create AppState

  **API/Type References**:
  - `crates/antigravity-core/src/proxy/common/thinking_config.rs` — `update_thinking_budget_config(ThinkingBudgetConfig)` (Task 1)

  **WHY Each Reference Matters**:
  - `accessors.rs:120` — Exact insertion point for hot reload. Must go BEFORE the write guard block (lines 124-136) to avoid holding the global static write during AppState lock acquisition.
  - `main.rs` — Must find the ProxyConfig construction site to know where to insert startup init

  **Acceptance Criteria**:

  - [ ] `main.rs` calls `update_thinking_budget_config()` during startup with ProxyConfig.thinking_budget
  - [ ] `hot_reload_proxy_config()` calls `update_thinking_budget_config()` after loading new config
  - [ ] `cargo build -p antigravity-server` compiles successfully
  - [ ] `cargo clippy --workspace -- -Dwarnings` passes

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Server compiles with wiring
    Tool: Bash (cargo)
    Steps:
      1. cargo check -p antigravity-server 2>&1
      2. Assert: exit code 0
      3. cargo clippy --workspace -- -Dwarnings 2>&1
      4. Assert: exit code 0
    Expected Result: Clean compilation with new wiring
    Evidence: Compiler output
  ```

  **Commit**: YES
  - Message: `feat(server): wire thinking budget config into startup and hot-reload`
  - Files: `main.rs`, `accessors.rs`
  - Pre-commit: `cargo check -p antigravity-server && cargo clippy --workspace -- -Dwarnings`

---

- [ ] 6. Integration Tests + Full Verification

  **What to do**:

  Write integration-level tests that verify the complete flow: config → global static → generation config output. Also run full workspace verification.

  **6a. Claude generation config tests** (add to existing test patterns):
  - Create tests in `crates/antigravity-core/src/proxy/mappers/claude/request/generation_config.rs` as inline `#[cfg(test)] mod tests`:
    - `test_auto_mode_no_client_thinking` — Auto mode, no client thinking → budget 16000, maxOutputTokens 48768
    - `test_auto_mode_with_client_budget` — Auto mode, client budget_tokens=5000 → budget 5000
    - `test_custom_mode_overrides_client` — Custom(30000), client budget=5000 → budget 30000
    - `test_adaptive_mode_sets_budget_minus_one` — Adaptive → thinkingBudget -1, maxOutputTokens 131072
    - `test_passthrough_mode_uses_client_budget` — Passthrough, client budget=8000 → budget 8000

  **6b. OpenAI generation config tests** (add to existing test patterns):
  - Create tests in `crates/antigravity-core/src/proxy/mappers/openai/request/generation_config.rs` as inline `#[cfg(test)] mod tests`:
    - `test_openai_auto_mode_default` — Auto, no max_tokens → budget 16000, maxOutputTokens 48768
    - `test_openai_custom_mode` — Custom(20000) → budget 20000
    - `test_openai_adaptive_mode` — Adaptive → budget -1, maxOutputTokens 131072
    - `test_openai_auto_small_max_tokens` — Auto, max_tokens=5000 → maxOutputTokens = 16000 + 8192 = 24192

  **6c. Wrapper tests** (add to `wrapper_tests.rs`):
    - `test_adaptive_gemini3_replaces_budget_with_level` — thinkingBudget:-1 → thinkingLevel:HIGH
    - `test_adaptive_gemini3_low_effort` — effort=low → thinkingLevel:LOW
    - `test_adaptive_gemini2_keeps_budget_minus_one` — thinkingBudget:-1 preserved
    - `test_adaptive_sets_max_output_131072` — maxOutputTokens set to 131072
    - `test_non_adaptive_flash_cap_preserved` — Auto + Flash → cap at 24576

  **6d. Full workspace verification:**
  ```bash
  cargo test --workspace
  cargo clippy --workspace -- -Dwarnings
  cargo check --workspace
  ```

  **Must NOT do**:
  - Do NOT write end-to-end tests requiring running server (unit/integration only)
  - Do NOT modify production code in this task — tests only

  **Recommended Agent Profile**:
  - **Category**: `unspecified-low`
    - Reason: Test-writing only, no architectural decisions needed
  - **Skills**: `[]`

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 4 (final)
  - **Blocks**: None
  - **Blocked By**: Task 5

  **References**:

  **Test References**:
  - `crates/antigravity-core/src/proxy/mappers/openai/request/tests.rs` — Existing OpenAI test patterns: construct `OpenAIRequest`, call `transform_openai_request`, assert on `Value`
  - `crates/antigravity-core/src/proxy/mappers/claude/tests_request/request_tests.rs` — Existing Claude test patterns: construct `ClaudeRequest`, call `transform_claude_request_in`, assert on `Value`
  - `crates/antigravity-core/src/proxy/mappers/gemini/wrapper_tests.rs` — Existing wrapper test patterns

  **Pattern References**:
  - `crates/antigravity-core/src/proxy/mappers/claude/request/generation_config.rs` — File to add inline tests to
  - `crates/antigravity-core/src/proxy/mappers/openai/request/generation_config.rs` — File to add inline tests to

  **WHY Each Reference Matters**:
  - `tests.rs` / `request_tests.rs` — Show how to construct test request structs and what fields are required vs optional
  - `wrapper_tests.rs` — Show how to construct JSON body for `wrap_request` and assert on output

  **Acceptance Criteria**:

  - [ ] ≥14 new tests across 3 test locations
  - [ ] All 4 modes tested in Claude path
  - [ ] All 4 modes tested in OpenAI path
  - [ ] Adaptive + Gemini 3/2 tested in wrapper
  - [ ] Flash cap regression tested
  - [ ] `cargo test --workspace` passes (all existing + new tests)
  - [ ] `cargo clippy --workspace -- -Dwarnings` passes
  - [ ] `cargo check --workspace` passes

  **Agent-Executed QA Scenarios:**

  ```
  Scenario: Full workspace passes
    Tool: Bash (cargo)
    Preconditions: All tasks 1-5 complete
    Steps:
      1. cargo test --workspace 2>&1
      2. Assert: exit code 0, "test result: ok" for all crates
      3. Count total tests: grep "test result" output → ≥190 tests (170 existing + 20 new)
      4. cargo clippy --workspace -- -Dwarnings 2>&1
      5. Assert: exit code 0
      6. cargo check --workspace 2>&1
      7. Assert: exit code 0
    Expected Result: Clean workspace with all tests passing
    Evidence: Full terminal output captured

  Scenario: No regression in existing tests
    Tool: Bash (cargo)
    Steps:
      1. cargo test -p antigravity-core --lib 2>&1 | grep "test result"
      2. Assert: total tests ≥ 190 (170 existing + 20 new), 0 failures
    Expected Result: Zero regressions
    Evidence: Test count output
  ```

  **Commit**: YES
  - Message: `test(proxy): comprehensive tests for adaptive thinking budget modes`
  - Files: `generation_config.rs` (both), `wrapper_tests.rs`
  - Pre-commit: `cargo test --workspace && cargo clippy --workspace -- -Dwarnings`

---

## Commit Strategy

| After Task | Message | Files | Verification |
|------------|---------|-------|--------------|
| 1 | `feat(types): add ThinkingBudgetMode enum and ThinkingBudgetConfig struct` | thinking.rs, mod.rs, proxy.rs, thinking_config.rs, common/mod.rs | cargo test -p antigravity-types && cargo test -p antigravity-core --lib |
| 2+3 | `feat(proxy): mode-aware thinking budget in Claude and OpenAI generation config` | claude/generation_config.rs, openai/generation_config.rs | cargo test -p antigravity-core --lib |
| 4 | `feat(proxy): adaptive thinking mode in Gemini wrapper (thinkingLevel + budget -1)` | wrapper.rs, wrapper_tests.rs | cargo test -p antigravity-core --lib |
| 5 | `feat(server): wire thinking budget config into startup and hot-reload` | main.rs, accessors.rs | cargo check -p antigravity-server |
| 6 | `test(proxy): comprehensive tests for adaptive thinking budget modes` | generation_config.rs (both), wrapper_tests.rs | cargo test --workspace |

---

## Success Criteria

### Verification Commands
```bash
cargo check --workspace                        # Expected: success
cargo clippy --workspace -- -Dwarnings         # Expected: 0 warnings
cargo test -p antigravity-types                # Expected: all pass (existing + 6 new)
cargo test -p antigravity-core --lib           # Expected: all pass (170+ existing + ~14 new)
cargo test --workspace                         # Expected: all pass
```

### Final Checklist
- [ ] All "Must Have" present (4 modes, 3 paths, hot-reload, tests)
- [ ] All "Must NOT Have" absent (no signature changes, no UI, no AppState field)
- [ ] All tests pass across workspace
- [ ] Zero clippy warnings
- [ ] Backward compatible: existing gui_config.json without thinking_budget field works
- [ ] Files under 300 lines
