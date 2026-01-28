<div align="center">

# Antigravity Server

### 🚀 **100% Rust** AI-шлюз: Headless, Отказоустойчивый, Высокопроизводительный

<img src="public/icon.png" alt="Antigravity Logo" width="140" height="140" style="border-radius: 24px;">

[![Upstream](https://img.shields.io/badge/Upstream-v3.3.45-888?style=for-the-badge&logo=github)](https://github.com/lbjlaq/Antigravity-Manager)
[![Rust](https://img.shields.io/badge/100%25_Rust-dea584?style=for-the-badge&logo=rust&logoColor=black)](https://www.rust-lang.org/)
[![Leptos](https://img.shields.io/badge/Leptos-WASM-8B5CF6?style=for-the-badge)](https://leptos.dev/)
[![Axum](https://img.shields.io/badge/Axum-Server-3B82F6?style=for-the-badge)](https://github.com/tokio-rs/axum)
[![License](https://img.shields.io/badge/License-CC--BY--NC--SA--4.0-gray?style=for-the-badge)](LICENSE)

[English](README.md) | **Русский** | [Upstream 中文](https://github.com/lbjlaq/Antigravity-Manager)

</div>

---

## 📸 Скриншоты

<div align="center">

| Дашборд | Управление аккаунтами |
|:-------:|:---------------------:|
| ![Dashboard](public/screenshot-dashboard.png) | ![Accounts](public/screenshot-accounts.png) |
| Мониторинг квот всех аккаунтов в реальном времени | Отслеживание квот по моделям с классификацией тиров |

</div>

---

**Antigravity Server** — высокопроизводительный AI-шлюз, который преобразует веб-сессии Google и Anthropic в стандартизированные OpenAI-совместимые API.

Полностью переработан для **headless-развёртывания на серверах** и **максимальной отказоустойчивости**. Это не форк, а архитектурно новый продукт на базе [Antigravity Manager](https://github.com/lbjlaq/Antigravity-Manager), созданный для VPS и Docker.

## 🎯 Зачем Antigravity Server?

[Antigravity Manager](https://github.com/lbjlaq/Antigravity-Manager) — отличное десктопное приложение. Antigravity Server создан для разработчиков, которым нужен **headless-демон** для работы на VPS, в Docker или как фоновый сервис с enterprise-уровнем стабильности.

### Ключевые отличия

| Функция | Antigravity Manager | Antigravity Server |
|---------|---------------------|-------------------|
| **Целевая платформа** | Desktop (Tauri + GUI) | **Headless-сервер (Axum)** |
| **Фронтенд** | React + TypeScript | **Leptos (100% Rust → WASM)** |
| **Архитектура** | Монолит | **Модульный Crate Workspace** |
| **Rate Limiting** | Реактивный (Retry на 429) | **AIMD Предиктивный алгоритм** |
| **Надёжность** | Базовый Failover | **Circuit Breakers на аккаунт** |
| **Роутинг** | Тихая подмена модели | **Строгий роутинг (явные ошибки)** |
| **Изоляция** | Общий IP | **WARP Proxy (IP на аккаунт)** |
| **Observability** | Локальный UI | **Resilience API + Prometheus** |

## ✨ Ключевые фичи

### 🖥️ Headless-сервер
Не требует X-сервер или GUI. Запускайте `antigravity-server` как лёгкий демон на любом Linux VPS. Встроенный веб-интерфейс на Leptos для удалённого управления.

### 📊 AIMD Предиктивный Rate Limiting
Алгоритм **Additive Increase / Multiplicative Decrease** (аналогично TCP congestion control) изучает оптимальную скорость запросов для каждого аккаунта. Предсказывает исчерпание квоты *до* того, как это произойдёт.

### 🛡️ Circuit Breakers и Resilience
Каждый аккаунт защищён независимым circuit breaker. При сбоях аккаунт автоматически изолируется. Мониторинг через **Resilience API**:
- `GET /api/resilience/health` — доступность аккаунтов
- `GET /api/resilience/circuits` — состояния circuit breakers
- `GET /api/resilience/aimd` — телеметрия rate limiting
- `GET /api/metrics` — Prometheus-совместимые метрики

### 🌐 Поддержка WARP Proxy
Уникальный IP для каждого аккаунта через Cloudflare WARP. Предотвращает корреляцию аккаунтов и IP-based rate limits.

## 🔌 Универсальный адаптер протоколов

Подключайте любой OpenAI-совместимый инструмент к Claude и Gemini:

```
┌─────────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│   Claude Code   │     │                     │     │  Google Gemini   │
│   OpenAI SDK    │ ──▶ │  Antigravity Proxy  │ ──▶ │  Anthropic API   │
│   Cursor / IDE  │     │   (localhost:8045)  │     │  (via OAuth)     │
│   Custom Bots   │     │                     │     │                  │
└─────────────────┘     └─────────────────────┘     └──────────────────┘
```

- **Стандартный API**: `/v1/chat/completions` и `/v1/messages`
- **Динамическое обнаружение**: `/v1/models` для интеграции с IDE
- **Генерация изображений**: Imagen 3 через OpenAI DALL-E интерфейс

## 🚀 Установка

### Через Nix (Рекомендуется)

```bash
git clone https://github.com/Stranmor/Antigravity-Server.git
cd Antigravity-Manager

nix run .#build-server
./target/release/antigravity-server
```

### Ручная сборка

Требуется Rust toolchain и [Trunk](https://trunkrs.dev/):

```bash
cargo build --release -p antigravity-server
./target/release/antigravity-server
```

## ⚡ Быстрый старт

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
    model="gemini-3-pro-high",
    messages=[{"role": "user", "content": "Привет!"}]
)
```

### cURL
```bash
curl http://127.0.0.1:8045/v1/chat/completions \
  -H "Authorization: Bearer sk-antigravity" \
  -H "Content-Type: application/json" \
  -d '{"model": "gemini-3-flash", "messages": [{"role": "user", "content": "Привет"}]}'
```

## 🔧 Деплой

### Systemd Service (Linux VPS)

`~/.config/systemd/user/antigravity.service`:
```ini
[Unit]
Description=Antigravity AI Gateway
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

### Переменные окружения
| Переменная | По умолчанию | Описание |
|------------|--------------|----------|
| `ANTIGRAVITY_PORT` | `8045` | Порт шлюза |
| `ANTIGRAVITY_DATA_DIR` | `~/.antigravity` | Путь к БД и конфигам |
| `RUST_LOG` | `info` | Уровень логирования |

## 📦 Структура проекта

```
crates/
├── antigravity-types/      # Базовые типы и иерархия ошибок
├── antigravity-core/       # Бизнес-логика (Proxy, AIMD, Circuits)
└── antigravity-client/     # Rust SDK (авто-обнаружение, ретраи, стриминг)

antigravity-server/         # Axum HTTP Entry Point
src-leptos/                 # 100% Rust WASM фронтенд
vendor/antigravity-upstream/ # Upstream (Git Submodule)
```

## 🔀 Стратегия синхронизации с Upstream

Используем **Semantic Porting** — не слепо копируем upstream, а выборочно интегрируем:

- ✅ **Всегда портируем**: баг-фиксы, поддержка новых моделей, security-патчи
- ❌ **Никогда не портируем**: React/Tauri код, изменения конфликтующие с нашим resilience-слоем

**🔄 Активная синхронизация**: Мы активно портируем все изменения upstream в течение 24-48 часов после релиза. Текущая синхронизация с v3.3.45, плюс наши эксклюзивные дополнения: AIMD предиктивный rate limiting, Circuit Breakers, Prometheus метрики, WARP proxy изоляция, Grace Retry и sticky session rebind при 429.

Подробная документация архитектуры: [AGENTS.md](AGENTS.md)

## 📄 Лицензия

Основан на [lbjlaq/Antigravity-Manager](https://github.com/lbjlaq/Antigravity-Manager). Благодарность оригинальным авторам за концепцию прокси. Antigravity Server — архитектурно новый продукт для headless-развёртывания.

**Лицензия**: [CC BY-NC-SA 4.0](LICENSE) — только некоммерческое использование.

<div align="center">

**Сделано с ❤️ на 100% Rust**

[![GitHub Stars](https://img.shields.io/github/stars/Stranmor/Antigravity-Server?style=social)](https://github.com/Stranmor/Antigravity-Server)

</div>
