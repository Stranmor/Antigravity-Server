# Draft: Adaptive Thinking Budget Port

## Requirements (confirmed)
- Port ThinkingBudgetConfig with 4 modes: Auto, Passthrough, Custom, Adaptive
- Wire into all 3 request paths (Claude, OpenAI, Gemini native)
- Make it runtime-configurable via ProxyConfig → hot-reloadable
- Default to Adaptive mode
- Adaptive mode: `thinkingBudget: -1` for Claude models, `thinkingLevel` for Gemini-3
- maxOutputTokens ceiling of 131072 for adaptive mode
- Keep existing constants as fallbacks for Auto/Custom modes

## Technical Decisions
- **Config location**: New file `crates/antigravity-types/src/models/config/thinking.rs` (not session.rs — session.rs at 119 lines but thinking has its own domain with enum + struct)
- **Wire through**: Follow ExperimentalConfig pattern → field on ProxyConfig, Arc<RwLock<>> in both AppStates, hot-reload in accessors.rs
- **API threading**: `build_generation_config()` in both Claude and OpenAI paths needs ThinkingBudgetConfig parameter. Currently takes pure data — will take &ThinkingBudgetConfig
- **Model detection**: Use ModelFamily::from_model_name() for Claude vs Gemini branching (SPOT)

## Research Findings

### Current Architecture (3 paths)
1. **Claude path** (`monolith.rs` → `claude/generation_config.rs`):
   - Respects client's `thinking.budget_tokens` if present
   - Auto-injects `THINKING_BUDGET (16000)` when no client config
   - maxOutputTokens: `budget + THINKING_OVERHEAD (32768)`
   
2. **OpenAI path** (`openai/mod.rs` → `openai/generation_config.rs`):
   - Always hardcodes `THINKING_BUDGET (16000)` — ignores client
   - maxOutputTokens: `THINKING_BUDGET + THINKING_OVERHEAD (48768)` when no client max_tokens
   - Or `THINKING_BUDGET + THINKING_MIN_OVERHEAD (24192)` when client max_tokens too small
   
3. **Gemini native** (`wrapper.rs`):
   - Passthrough — client controls everything
   - Only intervention: Flash model cap to 24576

### Two AppState layers
- Server AppState (`antigravity-server/src/state/mod.rs`): owns Arc<RwLock<T>> references
- Proxy AppState (`antigravity-core/src/proxy/server.rs`): receives same Arc references via build_proxy_router_with_shared_state()
- Hot-reload: accessors.rs acquires all write guards in alphabetical order

### Key files that need changes:
- `crates/antigravity-types/src/models/config/thinking.rs` (NEW)
- `crates/antigravity-types/src/models/config/mod.rs` (add re-export)
- `crates/antigravity-types/src/models/config/proxy.rs` (add field)
- `crates/antigravity-types/src/models/mod.rs` (add re-export)
- `crates/antigravity-core/src/proxy/common/thinking_constants.rs` (keep, but add Default impl for config)
- `crates/antigravity-core/src/proxy/mappers/claude/request/generation_config.rs` (accept config param)
- `crates/antigravity-core/src/proxy/mappers/openai/request/generation_config.rs` (accept config param)
- `crates/antigravity-core/src/proxy/mappers/gemini/wrapper.rs` (use config for Flash cap)
- `crates/antigravity-core/src/proxy/mappers/claude/request/monolith.rs` (pass config to build_generation_config)
- `crates/antigravity-core/src/proxy/mappers/openai/request/mod.rs` (pass config to build_generation_config)
- `crates/antigravity-core/src/proxy/server.rs` (add thinking_budget to proxy AppState)
- `antigravity-server/src/state/mod.rs` (add thinking_budget_config to AppStateInner)
- `antigravity-server/src/state/accessors.rs` (add to hot_reload)
- `crates/antigravity-core/src/proxy/handlers/claude/messages.rs` (read config, pass to transform)
- `crates/antigravity-core/src/proxy/handlers/openai/chat/` (read config, pass to transform)

## Open Questions
- None remaining — all resolved from research

## Scope Boundaries
- INCLUDE: ThinkingBudgetConfig type, 4 modes, wire through all 3 paths, hot-reload, tests
- EXCLUDE: Dedicated REST API endpoint for thinking config (use existing POST /api/config). UI changes. Unifying is_thinking_model detection (separate concern).
