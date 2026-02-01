<div align="center">

# Antigravity Server

**Production-grade AI Gateway — OpenAI-compatible API for Gemini & Claude**

<img src="public/icon.png" alt="Antigravity" width="120" height="120">

[![Rust](https://img.shields.io/badge/Rust-100%25-dea584?style=flat-square&logo=rust&logoColor=black)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/Axum-Backend-3B82F6?style=flat-square)](https://github.com/tokio-rs/axum)
[![Leptos](https://img.shields.io/badge/Leptos-WASM_UI-8B5CF6?style=flat-square)](https://leptos.dev/)
[![Upstream](https://img.shields.io/badge/Upstream-v4.0.7-888?style=flat-square)](https://github.com/lbjlaq/Antigravity-Manager)
[![License](https://img.shields.io/badge/License-CC--BY--NC--SA--4.0-gray?style=flat-square)](LICENSE)

[English](README.md) · [Русский](README_RU.md) · [Upstream 中文](https://github.com/lbjlaq/Antigravity-Manager)

</div>

---

Headless Rust server that transforms Google AI Studio and Anthropic Console web sessions into standard OpenAI-compatible APIs. Deploy on VPS, run as systemd daemon, manage via CLI or Web UI.

## Quick Start

```bash
# Build
cargo build --release -p antigravity-server

# Run
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
    model="gemini-3-pro-high",
    messages=[{"role": "user", "content": "Hello!"}]
)
```

```bash
# Claude Code CLI
export ANTHROPIC_API_KEY="sk-antigravity"
export ANTHROPIC_BASE_URL="http://127.0.0.1:8045"
claude
```

---

## Why This Fork?

| Capability | Upstream (Tauri) | This Fork (Axum) |
|------------|------------------|------------------|
| Deployment | Desktop GUI | **Headless VPS daemon** |
| Frontend | React + TypeScript | **Leptos (Rust → WASM)** |
| Rate Limiting | Reactive (retry on 429) | **AIMD predictive** |
| Reliability | Basic failover | **Circuit breakers + health scores** |
| Persistence | Direct file I/O | **Actor loop (race-free)** |
| Observability | Local UI only | **Prometheus metrics + REST API** |
| Audio/Video | Not supported | **Full multimodal** |

---

## Architecture

```
┌─────────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│  Claude Code    │     │                     │     │  Google Gemini   │
│  OpenAI SDK     │ ──▶ │  Antigravity Proxy  │ ──▶ │  Anthropic API   │
│  Cursor / IDE   │     │   (localhost:8045)  │     │  (via OAuth)     │
└─────────────────┘     └─────────────────────┘     └──────────────────┘
```

**Endpoints:**
- `POST /v1/chat/completions` — OpenAI-compatible chat
- `POST /v1/messages` — Anthropic-compatible messages
- `GET /v1/models` — Available models
- `POST /v1/images/generations` — Imagen 3 (DALL-E compatible)
- `POST /v1/audio/transcriptions` — Whisper-compatible

**Resilience API:**
- `GET /api/resilience/health` — Account availability
- `GET /api/resilience/circuits` — Circuit breaker states
- `GET /api/resilience/aimd` — Rate limit telemetry
- `GET /api/metrics` — Prometheus metrics

---

## Multimodal

**Audio** (official OpenAI format):
```python
response = client.chat.completions.create(
    model="gemini-3-pro",
    messages=[{
        "role": "user",
        "content": [
            {"type": "text", "text": "Transcribe this audio"},
            {"type": "input_audio", "input_audio": {"data": audio_b64, "format": "wav"}}
        ]
    }]
)
```

**Video** (Gemini extension):
```python
response = client.chat.completions.create(
    model="gemini-3-pro",
    messages=[{
        "role": "user",
        "content": [
            {"type": "text", "text": "Describe this video"},
            {"type": "video_url", "video_url": {"url": f"data:video/mp4;base64,{video_b64}"}}
        ]
    }]
)
```

Supported: `wav`, `mp3`, `ogg`, `flac`, `m4a` | `mp4`, `mov`, `webm`, `avi`

---

## CLI

```bash
antigravity-server account list          # List accounts with quotas
antigravity-server account add --file x  # Add account from JSON
antigravity-server account refresh all   # Refresh all quotas
antigravity-server warmup --all          # Warmup sessions
antigravity-server status                # Proxy statistics
antigravity-server config show           # Current config
```

---

## Deployment

**Systemd (user service):**

```ini
# ~/.config/systemd/user/antigravity.service
[Unit]
Description=Antigravity AI Gateway
After=network.target

[Service]
ExecStart=%h/.cargo/bin/antigravity-server
Restart=always
Environment=RUST_LOG=info
Environment=ANTIGRAVITY_PORT=8045

[Install]
WantedBy=default.target
```

```bash
systemctl --user enable --now antigravity
```

**Environment:**

| Variable | Default | Description |
|----------|---------|-------------|
| `ANTIGRAVITY_PORT` | `8045` | Server port |
| `ANTIGRAVITY_DATA_DIR` | `~/.antigravity_tools` | Data directory |
| `RUST_LOG` | `info` | Log level |

---

## Project Structure

```
crates/
├── antigravity-types/    # Shared types, errors, models
├── antigravity-core/     # Business logic (proxy, AIMD, circuits)
└── antigravity-client/   # Rust SDK (auto-discovery, streaming)

antigravity-server/       # Axum HTTP server + CLI
src-leptos/               # Leptos WASM frontend
vendor/antigravity-upstream/  # Upstream reference (submodule)
```

---

## License

Based on [lbjlaq/Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager). 

**License:** [CC BY-NC-SA 4.0](LICENSE) — Non-commercial use only.

<div align="center">

**Built with Rust**

</div>

