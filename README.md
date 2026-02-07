<div align="center">

# Antigravity Server

**Production-grade AI proxy built for agentic coding workloads**

<img src="public/icon.png" alt="Antigravity" width="120" height="120">

[![Rust](https://img.shields.io/badge/Rust-100%25-dea584?style=flat-square&logo=rust&logoColor=black)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/Axum-Backend-3B82F6?style=flat-square)](https://github.com/tokio-rs/axum)
[![Leptos](https://img.shields.io/badge/Leptos-WASM_UI-8B5CF6?style=flat-square)](https://leptos.dev/)
[![Fork](https://img.shields.io/badge/Fork_of-lbjlaq-888?style=flat-square)](https://github.com/lbjlaq/Antigravity-Manager)

</div>

---

## What is this?

A headless Rust server that turns [Antigravity](https://github.com/lbjlaq/Antigravity-Manager) accounts into a reliable, production-grade API — purpose-built for **agentic coding** (Claude Code, Cursor, Cline, OpenCode) where sessions run 50–200+ tool calls without human intervention.

**You provide:** Antigravity account credentials  
**You get:** OpenAI + Anthropic compatible API that survives rate limits, rotates accounts, and keeps thinking mode stable across long tool-use chains

```
┌─────────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│  Claude Code    │     │  Antigravity Server │     │                  │
│  Cursor / Cline │ ──► │  ┌───────────────┐  │ ──► │  Google Gemini   │
│  OpenCode       │     │  │ AIMD throttle │  │     │  (via OAuth)     │
│  Any OpenAI SDK │     │  │ Circuit break │  │     │                  │
│                 │     │  │ Sticky session│  │     └──────────────────┘
└─────────────────┘     │  └───────────────┘  │
                        └─────────────────────┘
```

---

## Quick Start

```bash
cargo build --release -p antigravity-server
./target/release/antigravity-server
# → http://127.0.0.1:8045
```

### Claude Code

```bash
export ANTHROPIC_BASE_URL=http://127.0.0.1:8045
export ANTHROPIC_API_KEY=sk-antigravity
claude   # thinking mode works, tool calls stay stable
```

### Cursor / Cline / OpenCode

Set base URL to `http://127.0.0.1:8045/v1` with any API key.

### Python / OpenAI SDK

```python
import openai

client = openai.OpenAI(
    api_key="sk-antigravity",
    base_url="http://127.0.0.1:8045/v1"
)

response = client.chat.completions.create(
    model="gemini-2.5-pro",
    messages=[{"role": "user", "content": "Hello!"}]
)
```

---

## Why This Fork?

The upstream [Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager) is a **Tauri desktop app** designed for manual chat. This fork is a **headless server** rebuilt for **unattended, high-throughput agentic workloads**.

### Architecture

| | Upstream (Tauri Desktop) | This Fork (Axum Server) |
|---|---|---|
| **Runtime** | Desktop GUI app (Tauri + React) | Headless VPS/server daemon |
| **Frontend** | React + TypeScript | Leptos (Rust → WASM, single binary) |
| **Crate structure** | Single `src-tauri/` (49k LOC) | Modular workspace: `core`, `types`, `server` (36k LOC) |
| **Deployment** | Install on desktop | `systemctl`, Docker, bare metal |

### Resilience & Routing — built for agentic

| Feature | Upstream | This Fork |
|---|---|---|
| **Rate limiting** | Retry on 429 | **AIMD** predictive throttle (TCP-congestion inspired) |
| **Account selection** | Round-robin | **Least-connections** + tier priority + health scores |
| **Failover** | Basic retry | **Circuit breakers** (per-account, per-endpoint) |
| **Session continuity** | None | **Sticky sessions** — account stays bound to conversation |
| **Thinking mode** | Disables on mixed history ⚠️ | **Always-on** — upstream handles mixed history natively |
| **Signature validation** | Silent downgrade | **Explicit error** — client knows when quality degrades |
| **Response validation** | None | **Thinking degradation detection** on every response |
| **Schema caching** | None | **Atomic schema cache** with hit/miss tracking |
| **Observability** | GUI dashboard | **Prometheus metrics** + structured JSON logs |

---

## API

**OpenAI-compatible:**
- `POST /v1/chat/completions` — Chat (streaming supported)
- `POST /v1/images/generations` — Imagen 3
- `POST /v1/audio/transcriptions` — Whisper-compatible
- `GET /v1/models` — Available models

**Anthropic-compatible:**
- `POST /v1/messages` — Claude messages API (full thinking + signature support)

**Management:**
- `GET /api/resilience/health` — Account health + circuit breaker status
- `GET /api/resilience/aimd` — AIMD rate limit state
- `GET /api/metrics` — Prometheus metrics

---

## CLI

```bash
antigravity-server account list          # List accounts
antigravity-server account add -f x.json # Import account
antigravity-server account refresh all   # Refresh quotas
antigravity-server status                # Server stats
```

---

## Deployment

```ini
# ~/.config/systemd/user/antigravity.service
[Unit]
Description=Antigravity Server
After=network.target

[Service]
ExecStart=%h/.local/bin/antigravity-server
Restart=always
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
```

```bash
systemctl --user enable --now antigravity
```

| Variable | Default | Description |
|----------|---------|-------------|
| `ANTIGRAVITY_PORT` | `8045` | Server port |
| `ANTIGRAVITY_DATA_DIR` | `~/.antigravity_tools` | Data directory |
| `RUST_LOG` | `info` | Log level (`debug` for signature tracing) |

---

## License

Fork of [lbjlaq/Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager).  
License: [CC BY-NC-SA 4.0](LICENSE) — Non-commercial use only.

---

<sub>⚠️ This README was generated by Claude Opus. The repository owner has no idea what's written here.</sub>

