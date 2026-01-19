<div align="center">

# Antigravity Server

### 🚀 **Pure Rust** AI Gateway: Headless, Resilient, High-Performance

<img src="public/icon.png" alt="Antigravity Logo" width="140" height="140" style="border-radius: 24px;">

[![Upstream](https://img.shields.io/badge/Upstream-v3.3.43-888?style=for-the-badge&logo=github)](https://github.com/lbjlaq/Antigravity-Manager)
[![Rust](https://img.shields.io/badge/100%25_Rust-dea584?style=for-the-badge&logo=rust&logoColor=black)](https://www.rust-lang.org/)
[![Leptos](https://img.shields.io/badge/Leptos-WASM-8B5CF6?style=for-the-badge)](https://leptos.dev/)
[![Axum](https://img.shields.io/badge/Axum-Server-3B82F6?style=for-the-badge)](https://github.com/tokio-rs/axum)
[![License](https://img.shields.io/badge/License-CC--BY--NC--SA--4.0-gray?style=for-the-badge)](LICENSE)

**English** | [Русский](README_RU.md) | [Upstream 中文](https://github.com/lbjlaq/Antigravity-Manager)

---

**Antigravity Server** is a high-performance AI gateway that transforms Google and Anthropic web sessions into standardized, OpenAI-compatible APIs. 

Built from the ground up for **headless server deployment** and **maximum resilience**, it's a complete architectural reimagining of the original [Antigravity Manager](https://github.com/lbjlaq/Antigravity-Manager) — not just a fork, but a production-ready server designed for VPS and Docker environments.

</div>

---

## 🎯 Why Antigravity Server?

While [Antigravity Manager](https://github.com/lbjlaq/Antigravity-Manager) provides an excellent desktop experience, Antigravity Server is built for developers who need a **headless daemon** that can run on a VPS, in Docker, or as a background service with enterprise-grade stability.

### Key Differentiators

| Feature | Antigravity Manager | Antigravity Server |
|---------|---------------------|-------------------|
| **Primary Target** | Desktop (Tauri + GUI) | **Headless Server (Axum Daemon)** |
| **Frontend** | React + TypeScript | **Leptos (Pure Rust → WASM)** |
| **Architecture** | Monolithic | **Modular Crate Workspace** |
| **Rate Limiting** | Reactive (Retry on 429) | **AIMD Predictive Algorithm** |
| **Reliability** | Basic Failover | **Circuit Breakers per Account** |
| **Routing** | Silent Model Substitution | **Strict Routing (Explicit Errors)** |
| **Isolation** | Shared IP | **WARP Proxy Support (Per-Account IP)** |
| **Observability**| Local UI | **Resilience API & Prometheus Metrics** |

---

## ✨ Killer Features

### 🖥️ Headless Server (Killer Feature #1)
No X server, no GUI required. Deploy `antigravity-server` as a lightweight daemon on any Linux VPS. It comes with a built-in Leptos-based Web UI for remote management.

### 📊 AIMD Predictive Rate Limiting (Killer Feature #2)
Using the **Additive Increase / Multiplicative Decrease** algorithm (similar to TCP congestion control), the gateway learns the optimal request rate for each account. It predicts quota exhaustion *before* it happens, ensuring zero wasted requests and smoother failover.

### 🛡️ Circuit Breakers & Resilience
Each account is protected by an independent circuit breaker. If an account starts failing, it's automatically isolated to prevent cascading failures. Monitor everything via the **Resilience API**:
- `GET /api/resilience/health` — Real-time account availability.
- `GET /api/resilience/circuits` — Circuit breaker states.
- `GET /api/resilience/aimd` — Rate limiting telemetry.
- `GET /api/metrics` — Prometheus-compatible metrics.

### 🌐 WARP Proxy Support
Avoid account correlation by assigning unique IPs to each account via Cloudflare WARP. Perfect for maintaining high reputation scores and avoiding broad IP-based rate limits.

---

## 🔌 Universal Protocol Adapter

Connect any OpenAI-compatible tool to Claude and Gemini:

```
┌─────────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│   Claude Code   │     │                     │     │  Google Gemini   │
│   OpenAI SDK    │ ──▶ │  Antigravity Proxy  │ ──▶ │  Anthropic API   │
│   Cursor / IDE  │     │   (localhost:8045)  │     │  (via OAuth)     │
│   Custom Bots   │     │                     │     │                  │
└─────────────────┘     └─────────────────────┘     └──────────────────┘
```

- **Standardized API**: Implements `/v1/chat/completions` and `/v1/messages`.
- **Dynamic Discovery**: Supports `/v1/models` for seamless integration with IDEs.
- **Image Support**: Imagen 3 via OpenAI DALL-E interface compatibility.

---

## 🚀 Installation

### Using Nix (Recommended)

The easiest way to build and run the server with all dependencies pinned:

```bash
git clone https://github.com/Stranmor/Antigravity-Manager.git
cd Antigravity-Manager

# Build and run the headless server
nix run .#build-server
./target/release/antigravity-server
```

### Manual Build

Requires Rust toolchain and [Trunk](https://trunkrs.dev/) for the frontend:

```bash
# Build the server (automatically builds the Leptos UI via build.rs)
cargo build --release -p antigravity-server

# Run
./target/release/antigravity-server
```

---

## ⚡ Quick Start

### Claude Code CLI
```bash
export ANTHROPIC_API_KEY="sk-antigravity"
export ANTHROPIC_BASE_URL="http://127.0.0.1:8045"
claude
```

### Python (OpenAI SDK)
```python
import openai

client = openai.OpenAI(
    api_key="sk-antigravity",
    base_url="http://127.0.0.1:8045/v1"
)

response = client.chat.completions.create(
    model="gemini-2.0-pro", # Automatically routed to best account
    messages=[{"role": "user", "content": "Hello!"}]
)
```

### cURL
```bash
curl http://127.0.0.1:8045/v1/chat/completions \
  -H "Authorization: Bearer sk-antigravity" \
  -H "Content-Type: application/json" \
  -d '{"model": "gemini-2.5-flash", "messages": [{"role": "user", "content": "Hi"}]}'
```

---

## 🔧 Deployment

### Systemd Service (Linux VPS)
Create `~/.config/systemd/user/antigravity.service`:

```ini
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

### Environment Variables
| Variable | Default | Description |
|----------|---------|-------------|
| `ANTIGRAVITY_PORT` | `8045` | Port the gateway listens on |
| `ANTIGRAVITY_DATA_DIR` | `~/.antigravity` | Path for database and configuration |
| `RUST_LOG` | `info` | Logging verbosity (debug, info, warn) |

---

## 📦 Project Structure

```
crates/
├── antigravity-types/      # Foundation types & error hierarchy
├── antigravity-shared/     # Re-export layer for external crates
├── antigravity-core/       # Business logic (Proxy, AIMD, Circuits)
└── antigravity-server/     # Axum HTTP Entry Point

src-leptos/                 # Pure Rust WASM Frontend
vendor/antigravity-upstream/ # Upstream reference (Git Submodule)
```

---

## 🔀 Upstream Sync Strategy

This fork uses **Semantic Porting** — we don't blindly copy upstream changes. Instead, we:

- ✅ **Always Port**: Bug fixes, new model support, security patches, JSON schema improvements
- ❌ **Never Port**: React/Tauri code (we use Leptos/Axum), changes conflicting with our resilience layer

**Current Sync**: We track upstream v3.3.43 while maintaining our custom additions (AIMD, Circuit Breakers, Prometheus, WARP support).

See [AGENTS.md](AGENTS.md) for detailed architecture documentation and sync workflow.

---

## 📄 License & Attribution

This project is based on [lbjlaq/Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager). Full credit to the original authors for the core proxy concept. Antigravity Server is a complete architectural reimagining focused on headless deployment and resilience.

**License**: [CC BY-NC-SA 4.0](LICENSE) — Non-commercial use only.

---

<div align="center">

**Built with ❤️ in 100% Rust**

[![GitHub Stars](https://img.shields.io/github/stars/Stranmor/Antigravity-Manager?style=social)](https://github.com/Stranmor/Antigravity-Manager)

</div>
