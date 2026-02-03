<div align="center">

# Antigravity Server

**Headless proxy for Antigravity accounts with OpenAI-compatible API**

<img src="public/icon.png" alt="Antigravity" width="120" height="120">

[![Rust](https://img.shields.io/badge/Rust-100%25-dea584?style=flat-square&logo=rust&logoColor=black)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/Axum-Backend-3B82F6?style=flat-square)](https://github.com/tokio-rs/axum)
[![Leptos](https://img.shields.io/badge/Leptos-WASM_UI-8B5CF6?style=flat-square)](https://leptos.dev/)
[![Upstream](https://img.shields.io/badge/Upstream-lbjlaq-888?style=flat-square)](https://github.com/lbjlaq/Antigravity-Manager)

</div>

---

## What is this?

A headless Rust server that proxies AI requests through [Antigravity](https://github.com/lbjlaq/Antigravity-Manager) accounts (Google AI Studio / Anthropic Console OAuth sessions).

**You provide:** Antigravity account JSON files (exported from upstream desktop app)  
**You get:** OpenAI-compatible API endpoint for Gemini & Claude models

```
┌─────────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│  Claude Code    │     │                     │     │  Google Gemini   │
│  OpenAI SDK     │ ──► │  Antigravity Server │ ──► │  Anthropic API   │
│  Cursor / IDE   │     │   (localhost:8045)  │     │  (via OAuth)     │
└─────────────────┘     └─────────────────────┘     └──────────────────┘
```

---

## Quick Start

```bash
cargo build --release -p antigravity-server
./target/release/antigravity-server
# → http://127.0.0.1:8045
```

```python
import openai

client = openai.OpenAI(
    api_key="sk-antigravity",
    base_url="http://127.0.0.1:8045/v1"
)

response = client.chat.completions.create(
    model="gemini-3-pro",
    messages=[{"role": "user", "content": "Hello!"}]
)
```

---

## Why This Fork?

| Feature | Upstream (Tauri Desktop) | This Fork (Axum Server) |
|---------|--------------------------|-------------------------|
| Deployment | Desktop GUI app | Headless VPS daemon |
| Frontend | React + TypeScript | Leptos (Rust WASM) |
| Rate Limiting | Retry on 429 | AIMD predictive throttling |
| Account Selection | Round-robin | Least-connections + tier priority |
| Failover | Basic retry | Circuit breakers + health scores |
| Multimodal | — | Audio, video, images |
| Observability | Local UI | Prometheus metrics + REST API |

---

## API

**OpenAI-compatible:**
- `POST /v1/chat/completions` — Chat (streaming supported)
- `POST /v1/images/generations` — Imagen 3
- `POST /v1/audio/transcriptions` — Whisper-compatible
- `GET /v1/models` — Available models

**Anthropic-compatible:**
- `POST /v1/messages` — Claude messages API

**Management:**
- `GET /api/resilience/health` — Account status
- `GET /api/resilience/aimd` — Rate limit stats
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
ExecStart=%h/.cargo/bin/antigravity-server
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

---

## License

Fork of [lbjlaq/Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager).  
License: [CC BY-NC-SA 4.0](LICENSE) — Non-commercial use only.
