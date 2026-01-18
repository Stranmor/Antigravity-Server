# Contributing to Antigravity Manager (Stranmor Fork)

Thank you for your interest in contributing!

## 🏗️ Project Architecture

This is a **fork** of [lbjlaq/Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager) with significant architectural changes. Before contributing, please understand the key differences:

| Component | Upstream | This Fork |
|-----------|----------|-----------|
| Backend | Tauri (desktop) | **Axum (headless server)** |
| Frontend | React + TypeScript | **Leptos (Rust → WASM)** |
| Rate Limiting | Reactive | **AIMD Predictive** |
| Reliability | Basic | **Circuit Breakers** |

**Important**: We don't blindly merge upstream. We use **Semantic Porting** — selectively integrating useful changes while maintaining our architecture.

## 🛠️ Development Setup

```bash
# Clone with submodules
git clone --recurse-submodules https://github.com/Stranmor/Antigravity-Manager.git
cd Antigravity-Manager

# Using Nix (recommended)
nix develop

# Or manual setup (requires Rust 1.75+, Trunk)
cargo build --workspace
```

### Pre-commit Hooks

We use pre-commit hooks for quality. They run automatically:
- `cargo fmt --check`
- `cargo clippy --workspace -- -D warnings`

## 📝 Guidelines

### Code Style
- **100% Rust** — no TypeScript/Python
- **Clippy clean** — `cargo clippy -- -D warnings` must pass
- **No `#[allow(...)]`** — fix warnings, don't suppress them
- **Type-safe** — no `as any` equivalents, no stringly-typed code

### What We Accept
- ✅ Bug fixes with tests
- ✅ Performance improvements with benchmarks
- ✅ New resilience features (circuit breakers, rate limiting)
- ✅ Documentation improvements
- ✅ Leptos UI improvements

### What We Don't Accept
- ❌ React/TypeScript code
- ❌ Changes that break headless operation
- ❌ Features requiring GUI/desktop environment
- ❌ Tauri-specific code

## 💬 Commit Messages

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(proxy): add circuit breaker for account isolation
fix(leptos): correct timestamp parsing in monitor
docs(readme): add Russian translation
refactor(core): extract AIMD into separate module
```

**Types**: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

## 🔀 Pull Request Process

1. Fork the repository
2. Create a feature branch: `git checkout -b feat/my-feature`
3. Make changes, ensure tests pass
4. Run pre-commit checks: `cargo fmt && cargo clippy -- -D warnings`
5. Commit with conventional message
6. Push and open PR

### PR Checklist
- [ ] Code compiles without warnings
- [ ] Clippy passes with `-D warnings`
- [ ] Tests pass (if applicable)
- [ ] Documentation updated (if applicable)

## 🔄 Upstream Sync

We track upstream in `vendor/antigravity-upstream/` (git submodule). If you're porting upstream features:

1. Update submodule: `git submodule update --remote`
2. Review changes: `git diff` in vendor/
3. **Semantically port** — don't copy-paste, adapt to our architecture
4. Test thoroughly

See [AGENTS.md](AGENTS.md) for detailed sync workflow.

---

Questions? Open an issue or discussion.
