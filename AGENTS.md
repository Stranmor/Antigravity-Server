
## 🚨 ARCHITECTURAL DECISION [2026-01-17]: Full Containerization Migration

**Problem:** Current deployment method via `install-server` copies a binary directly to the host, violating rootless containerization and reproducible deployment doctrines.

**Status:** PENDING — Container infrastructure prepared in flake.nix but manual verification needed.

---

## 🏛️ ARCHITECTURAL EVOLUTION PLAN v4.1 [2026-01-17]

**Status:** PHASE 1 COMPLETE — Type Extraction done. See `.gemini/architecture_evolution_plan.md` for full details.

### Completed Improvements:
- [x] **NEW: `antigravity-types` Crate** — Created foundation crate with:
  - `error/` — Typed error hierarchy (`AccountError`, `ProxyError`, `ConfigError`, `TypedError`)
  - `models/` — Domain models (`Account`, `TokenData`, `QuotaData`, `AppConfig`, `ProxyConfig`)
  - `protocol/` — API protocol types (`OpenAI`, `Claude`, `Gemini` message types)
  - All types are serde-serializable, Clone, and PartialEq
  - 7 unit tests passing
- [x] **Typed Errors** — Added `AccountError`, `ProxyError`, `ConfigError` to `antigravity-shared/src/error.rs`
- [x] **Clippy Compliance** — Full workspace passes `cargo clippy -- -Dwarnings`
- [x] **Doctrine-compliant Allows** — `#[allow(warnings)]` only on vendor-symlinked modules per WRAPPER DOCTRINE (2.11)
- [x] **Removed False Dead Code** — `#[allow(dead_code)]` removed from AIMD fields that are actually used
- [x] **Resilience API** — Added `/api/resilience/*` endpoints:
  - `GET /api/resilience/health` — Account health status
  - `GET /api/resilience/circuits` — Circuit breaker states
  - `GET /api/resilience/aimd` — AIMD rate limiting stats
- [x] **Architecture Documentation** — Created `.gemini/architecture_evolution_plan.md`
- [x] **Binary Deployed** — Server rebuilt and deployed to systemd service
- [x] **Fixed Missing Default** — Added `default_sticky_ttl()` function

### Next Steps (Ordered by Priority):
- [ ] **Phase 1b:** Wire `antigravity-types` into existing crates (deprecate duplicate types)
- [ ] **Phase 2:** Extract `antigravity-proxy` crate (COPY vendor code, not symlink)
- [ ] **Phase 3:** Extract `antigravity-accounts` crate (account management)
- [ ] **Phase 4:** Consolidate dual AppState into single definition
- [ ] **Phase 5:** Delete legacy code after migration complete

---

## 📊 Current Workspace Structure

```
crates/
├── antigravity-types/      # 🆕 NEW (Phase 1) — Foundation types
│   └── src/
│       ├── error/          # Typed error hierarchy
│       ├── models/         # Domain models
│       └── protocol/       # OpenAI/Claude/Gemini types
├── antigravity-core/       # Monolith (to be split in Phase 2-5)
│   └── src/proxy/
│       ├── [symlinks]     → #[allow(warnings)] per Wrapper Doctrine
│       └── [our files]    → Clippy STRICT (no allows)
├── antigravity-shared/     # Types + Errors (will merge into types)
│   └── src/
│       ├── error.rs
│       ├── models/
│       └── proxy/config
antigravity-server/         # HTTP entry point
├── src/
│   ├── api.rs             # /api/resilience/* endpoints
│   └── state.rs           # Server AppState
antigravity-vps-cli/        # CLI companion
src-leptos/                 # WebUI (WASM)
vendor/
└── antigravity-upstream/   # Git submodule (READ-ONLY)
```

---

## ✅ VERIFICATION STATUS

- `cargo check --workspace` ✓
- `cargo clippy --workspace -- -Dwarnings` ✓
- `cargo test -p antigravity-types` ✓ (7 tests passed)
- `cargo build --release -p antigravity-server` ✓
- `systemctl --user status antigravity-manager.service` ✓ (active running)
- `/api/resilience/*` endpoints respond correctly ✓

---
