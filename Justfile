set shell := ["bash", "-c"]

# Показать доступные команды
default:
    @just --list

# ============ НОВАЯ АРХИТЕКТУРА: Headless Server + WebUI ============

# Собрать headless сервер (рекомендуемый способ)
build-server:
    @echo "📦 Building Antigravity Server..."
    cd src-leptos && trunk build --release
    cargo build --release -p antigravity-server
    @echo "✅ Build complete: target/release/antigravity-server"

# Установить headless сервер и перезапустить сервис
install-server: build-server
    @echo "🚀 Installing Antigravity Server..."
    pkill -9 -f antigravity-server || true
    systemctl --user stop antigravity-manager || true
    
    cp target/release/antigravity-server ~/.local/bin/
    chmod +x ~/.local/bin/antigravity-server
    
    systemctl --user daemon-reload
    systemctl --user restart antigravity-manager
    @echo "✅ Installed and Service Started"
    @echo "🌐 WebUI available at: http://localhost:8045/"

# Запустить сервер в foreground (для дебага)
run-server:
    @echo "🚀 Starting Antigravity Server..."
    cd src-leptos && trunk build --release
    ANTIGRAVITY_STATIC_DIR=./src-leptos/dist cargo run --release -p antigravity-server

# Проверить статус сервиса
status:
    systemctl --user status antigravity-manager
    @echo ""
    @echo "API Status:"
    curl -s http://localhost:8045/api/status || echo "Server not running"

# ============ LEGACY: Tauri Desktop App (deprecated) ============

# Собрать Tauri app (устаревший способ)
build-tauri:
    @echo "⚠️  WARNING: Tauri app is deprecated. Use 'just build-server' instead."
    @echo "📦 Building Tauri Release Binary..."
    cargo tauri build
    @echo "✅ Build complete: target/release/antigravity_tools"

# ============ ОБЩИЕ КОМАНДЫ ============

# Полная очистка кешей
clean:
    @echo "🧹 Cleaning everything..."
    cargo clean
    rm -rf src-tauri/target
    rm -rf src-leptos/dist
    rm -rf src-leptos/target
    @echo "✨ Sparkle clean"

# Собрать только frontend (Leptos)
build-frontend:
    @echo "📦 Building Leptos Frontend..."
    cd src-leptos && trunk build --release
    @echo "✅ Frontend built: src-leptos/dist/"

# Обновить upstream (fetch + merge, без reset!)
sync-upstream:
    @echo "🔄 Syncing with Upstream..."
    git fetch upstream
    git merge upstream/main
    @echo "✅ Synced. If conflicts occurred, resolve them manually."

# Линтинг
lint:
    cargo clippy --workspace -- -D warnings

# Тесты
test:
    cargo test --workspace
