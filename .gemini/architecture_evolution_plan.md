# 🏛️ ARCHITECTURE EVOLUTION PLAN: Antigravity Manager v4.0

> **Document Version:** 1.1.0
> **Status:** PHASE 1 COMPLETE — Type Extraction Done
> **Created:** 2026-01-17
> **Updated:** 2026-01-17
> **Doctrine Alignment:** WRAPPER DOCTRINE (2.11), NO SHORTCUTS (2.11b), RUST ABSOLUTISM (2.5)


---

## 📋 EXECUTIVE SUMMARY

Текущая архитектура Antigravity Manager прошла через несколько эволюционных этапов: от Tauri Desktop App → Headless Daemon (`antigravity-server`). Основная логика proxy работает корректно, однако архитектура накопила технический долг, который необходимо систематически устранить.

### Ключевые проблемы:

1. **Symlink Hell** — симлинки на vendor усложняют понимание кода и CI/CD
2. **Scattered State** — состояние приложения размазано между `AppState` в server и `AppState` в core
3. **Missing Separation of Concerns** — proxy handlers содержат как бизнес-логику так и HTTP-специфику
4. **Suboptimal Crate Boundaries** — `antigravity-shared` слишком тонкий, а `antigravity-core` — монолит
5. **ALLOW Directive Violations** — `#[allow(clippy::all)]` на upstream модулях нарушает Clippy Absolutism

---

## 🗂️ CURRENT ARCHITECTURE ANALYSIS

### Workspace Structure
```
Antigravity-Manager/
├── antigravity-server/         # 🟢 Main entry point (headless daemon)
│   └── src/
│       ├── main.rs             # Axum server bootstrap  
│       ├── state.rs            # AppState (DUPLICATE of core!)
│       └── api.rs              # REST API endpoints
├── crates/
│   ├── antigravity-core/       # 🟡 Business logic (MONOLITH)
│   │   └── src/
│   │       ├── proxy/          # Mixed: symlinks + our code
│   │       │   ├── handlers/   → symlink to vendor
│   │       │   ├── mappers/    → symlink to vendor  
│   │       │   ├── common/     # Mixed folder
│   │       │   ├── server.rs   # OUR Axum router (has AppState)
│   │       │   ├── token_manager.rs  # OUR AIMD token manager
│   │       │   └── ...
│   │       ├── modules/        # Account, Config, OAuth, etc.
│   │       ├── models/         # Data types
│   │       └── utils/
│   └── antigravity-shared/     # 🔴 Too thin (only models)
├── vendor/
│   └── antigravity-upstream/   # Git submodule (read-only)
├── src-tauri/                  # Legacy Tauri app (read-only reference)
└── src-leptos/                 # WebUI frontend (WASM)
```

### Critical Issues Identified

#### 1. **Double AppState Anti-pattern**
```rust
// antigravity-server/src/state.rs
pub struct AppState { ... }  // One AppState here

// crates/antigravity-core/src/proxy/server.rs  
pub struct AppState { ... }  // ANOTHER AppState here (different fields!)
```
**Impact:** Confusion, maintenance burden, potential desync.

#### 2. **Symlink-based Module Inclusion**
```rust
// proxy/mod.rs
#[allow(clippy::all)]
#[allow(warnings)]
pub mod handlers;  // This is a symlink → vendor/
```
**Impact:** Can't run clippy properly, IDE confusion, CI fragility.

#### 3. **Monolithic `antigravity-core`**
- Contains everything: proxy logic, account management, config, OAuth, DB access
- 45k+ bytes of token_manager.rs alone
- Impossible to use proxy without account module

#### 4. **Missing Error Type Hierarchy**
```rust
// Current: scattered Result<T, String> everywhere
pub fn list_accounts() -> Result<Vec<Account>, String>

// Should be: typed errors
pub fn list_accounts() -> Result<Vec<Account>, AccountError>
```

---

## 🎯 TARGET ARCHITECTURE (v4.0)

### New Workspace Structure
```
Antigravity-Manager/
├── crates/
│   ├── antigravity-proxy/       # 🆕 PURE PROXY LOGIC (vendor overlay)
│   │   └── src/
│   │       ├── handlers/        # Copied (not symlinked!) from vendor
│   │       ├── mappers/         
│   │       ├── protocol/        # OpenAI/Claude/Gemini abstractions
│   │       ├── resilience/      # AIMD, CircuitBreaker, Health
│   │       └── lib.rs
│   │
│   ├── antigravity-accounts/    # 🆕 Account management
│   │   └── src/
│   │       ├── storage.rs       # Filesystem/DB abstraction
│   │       ├── token.rs         # TokenManager (rotation logic)
│   │       ├── oauth.rs         # OAuth flow
│   │       └── lib.rs
│   │
│   ├── antigravity-server/      # 🔄 HTTP server layer ONLY
│   │   └── src/
│   │       ├── routes/          # Axum route definitions
│   │       ├── state.rs         # THE ONLY AppState
│   │       └── main.rs
│   │
│   ├── antigravity-types/       # 🆕 Shared types (replaces antigravity-shared)
│   │   └── src/
│   │       ├── models/          # Account, Config, Quota, etc.
│   │       ├── error.rs         # Unified error types
│   │       ├── protocol/        # OpenAI/Claude/Gemini message types
│   │       └── lib.rs
│   │
│   └── antigravity-cli/         # 🔄 (was antigravity-vps-cli)
│       └── src/main.rs
│
├── vendor/
│   └── antigravity-upstream/    # Git submodule (REFERENCE ONLY)
│
└── src-leptos/                  # WebUI (unchanged)
```

### Dependency Graph
```
                    antigravity-types (base types, no deps)
                           │
          ┌────────────────┼────────────────┐
          ▼                ▼                ▼
   antigravity-proxy  antigravity-accounts  (other optional crates)
          │                │
          └────────┬───────┘
                   ▼
            antigravity-server
                   │
          ┌────────┴────────┐
          ▼                 ▼
      antigravity-cli    WebUI (WASM)
```

---

## 📝 IMPLEMENTATION PHASES

### Phase 1: Type Extraction (LOW RISK)
**Duration:** ~2 hours

1. Create `crates/antigravity-types/`
2. Move models from `antigravity-shared` and `antigravity-core/models`
3. Define proper error types:
```rust
// antigravity-types/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("Account not found: {0}")]
    NotFound(String),
    #[error("Token expired")]
    TokenExpired,
    #[error("Storage error: {0}")]
    Storage(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("Upstream unavailable: {provider}")]
    UpstreamUnavailable { provider: String },
    #[error("Rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    // ...
}
```

### Phase 2: Proxy Extraction (MEDIUM RISK)
**Duration:** ~4 hours

1. Create `crates/antigravity-proxy/`
2. **CRITICAL:** Copy (not symlink!) handler code from vendor
3. Apply WRAPPER DOCTRINE: our extensions wrap upstream logic
4. Remove all `#[allow(clippy::all)]` — fix warnings properly

```rust
// antigravity-proxy/src/handlers/claude.rs
// NOT a symlink! Copied and maintained as our version.
// Upstream changes ported via semantic review, not blind copy.

use antigravity_types::protocol::claude::*;
use crate::resilience::AIMDController;

pub async fn handle_messages(
    state: AppState,
    request: ClaudeRequest,
) -> Result<ClaudeResponse, ProxyError> {
    // Pre-request AIMD check
    state.aimd.before_request(&request.model).await?;
    
    // Forward to upstream
    let result = upstream::forward_claude(request).await;
    
    // Post-request AIMD feedback
    state.aimd.after_request(&result).await;
    
    result
}
```

### Phase 3: Account Extraction (MEDIUM RISK)
**Duration:** ~3 hours

1. Create `crates/antigravity-accounts/`
2. Move account management from `antigravity-core/modules/account.rs`
3. Define storage trait for testability:
```rust
// antigravity-accounts/src/storage.rs
#[async_trait]
pub trait AccountStorage: Send + Sync {
    async fn list(&self) -> Result<Vec<Account>, AccountError>;
    async fn get(&self, id: &str) -> Result<Account, AccountError>;
    async fn save(&self, account: &Account) -> Result<(), AccountError>;
    async fn delete(&self, id: &str) -> Result<(), AccountError>;
}

pub struct FileSystemStorage { path: PathBuf }
impl AccountStorage for FileSystemStorage { ... }

// Later: SQLite, Redis, etc.
```

### Phase 4: Server Consolidation (LOW RISK)
**Duration:** ~1 hour

1. Merge the two `AppState` structs
2. Move `antigravity-server/` into `crates/antigravity-server/`
3. Rename `antigravity-vps-cli` → `crates/antigravity-cli`

### Phase 5: Legacy Cleanup (FINAL)
**Duration:** ~1 hour

1. Delete `crates/antigravity-core/` (absorbed into new crates)
2. Delete `crates/antigravity-shared/` (replaced by `antigravity-types`)
3. Update `flake.nix` build scripts
4. Verify full test suite passes

---

## 🔧 IMMEDIATE QUICK WINS (Can Do Now)

Before the full refactor, these improvements can be applied immediately:

### 1. Remove Symlinks, Use Direct Copies
```bash
# Instead of symlinks, sync-upstream.sh copies files
# Files in crates/antigravity-core/src/proxy/ become real files
# We maintain them, porting upstream changes semantically
```

### 2. Fix `#[allow(...)]` Violations
- Remove all `#[allow(clippy::all)]` from proxy/mod.rs
- Fix each clippy warning properly
- Upstream code gets cleaned up as we own the copy now

### 3. Consolidate AppState
```rust
// Keep ONLY antigravity-server/src/state.rs
// proxy/server.rs should receive state as parameter, not define AppState
```

### 4. Add Tracing Spans for Observability
```rust
#[tracing::instrument(skip(state), fields(model = %request.model))]
pub async fn handle_chat_completions(
    State(state): State<AppState>,
    Json(request): Json<ChatRequest>,
) -> impl IntoResponse {
    // ...
}
```

---

## ⚠️ MIGRATION RISKS & MITIGATIONS

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Breaking upstream sync | MEDIUM | HIGH | Maintain vendor submodule as reference, semantic port only |
| API incompatibility | LOW | MEDIUM | Keep REST API unchanged, only internals change |
| WebUI breakage | LOW | LOW | Frontend uses REST API, backend changes transparent |
| Container build failure | MEDIUM | MEDIUM | Test `nix build .#antigravity-server-image` after each phase |

---

## 📊 SUCCESS METRICS

After migration complete:
- [ ] `cargo clippy --workspace -- -D warnings` passes (no allows)
- [ ] No symlinks in `crates/` directory tree
- [ ] Single `AppState` definition in workspace
- [ ] Each crate < 20 files, < 100KB total
- [ ] Test coverage > 60% for account/proxy logic
- [ ] Container image < 50MB compressed

---

## 🚀 READY TO PROCEED?

Рекомендация: **Начать с Phase 1 (Type Extraction)** — это минимально инвазивное изменение, которое создаёт фундамент для остальных фаз.

```bash
# Verify current state compiles
cargo check --workspace

# After Phase 1
cargo check -p antigravity-types

# Full verification
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

---

*Generated by Antigravity Architecture Audit System*
