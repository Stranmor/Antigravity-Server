
## 🚨 ARCHITECTURAL DECISION [2026-01-17]: Full Containerization Migration

**Problem:** Current deployment method via `install-server` copies a binary directly to the host, violating rootless containerization and reproducible deployment doctrines.

**Status:** PENDING — Container infrastructure prepared in flake.nix but manual verification needed.

---

## 🏛️ ARCHITECTURAL EVOLUTION PLAN v4.0 [2026-01-17]

**Status:** PHASE 1 COMPLETE — See `.gemini/architecture_evolution_plan.md` for full details.

### Completed Improvements:
- [x] **Typed Errors** — Added `AccountError`, `ProxyError`, `ConfigError` to `antigravity-shared/src/error.rs`
- [x] **Clippy Compliance** — Removed redundant `#[allow(clippy::all)]` directives
- [x] **Doctrine-compliant Allows** — `#[allow(warnings)]` only on vendor-symlinked modules per WRAPPER DOCTRINE (2.11)
- [x] **Removed False Dead Code** — `#[allow(dead_code)]` removed from AIMD fields that are actually used
- [x] **Resilience API** — Added `/api/resilience/*` endpoints:
  - `GET /api/resilience/health` — Account health status
  - `GET /api/resilience/circuits` — Circuit breaker states
  - `GET /api/resilience/aimd` — AIMD rate limiting stats
- [x] **Architecture Documentation** — Created `.gemini/architecture_evolution_plan.md`
- [x] **Binary Deployed** — Server rebuilt and deployed to systemd service

### Next Steps (Ordered by Priority):
- [ ] **Phase 2:** Extract `antigravity-proxy` crate (COPY vendor code, not symlink)
- [ ] **Phase 3:** Extract `antigravity-accounts` crate (account management)
- [ ] **Phase 4:** Consolidate AppState into single definition
- [ ] **Phase 5:** Delete legacy crates (`antigravity-core` split, `antigravity-shared` → `antigravity-types`)

---

## 📊 Current Workspace Structure

```
crates/
├── antigravity-core/       # Monolith (to be split in Phase 2-5)
│   └── src/proxy/
│       ├── [symlinks]     → #[allow(warnings)] per Wrapper Doctrine
│       └── [our files]    → Clippy STRICT (no allows)
├── antigravity-shared/     # Types + Errors
│   └── src/
│       ├── error.rs       ← NEW: typed errors
│       ├── models/
│       └── proxy/config
antigravity-server/         # HTTP entry point
├── src/
│   ├── api.rs             ← NEW: /api/resilience/* endpoints
│   └── state.rs           ← Cleaned up: no more #[allow(dead_code)]
antigravity-vps-cli/        # CLI companion
src-leptos/                 # WebUI (WASM)
vendor/
└── antigravity-upstream/   # Git submodule (READ-ONLY)
```

---

## ✅ VERIFICATION STATUS

- `cargo check --workspace` ✓
- `cargo clippy --workspace -- -D warnings` ✓
- `cargo build --release -p antigravity-server` ✓
- `systemctl --user status antigravity-manager.service` ✓ (active running)
- `/api/resilience/*` endpoints respond correctly ✓

---
