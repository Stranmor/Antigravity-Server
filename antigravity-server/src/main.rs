//! Antigravity Server - Headless Daemon
//!
//! A pure Rust HTTP server that:
//! - Runs the proxy logic (account rotation, API forwarding) on /v1/*
//! - Serves the Leptos WebUI as static files
//! - Provides a REST API for CLI and UI control on /api/*
//!
//! Access via: http://localhost:8045

use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit, http::StatusCode, response::IntoResponse, routing::get, Router,
};
use clap::Parser;
use listenfd::ListenFd;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod api;
mod cli;
mod commands;
mod scheduler;
mod state;

use antigravity_core::proxy::server::AxumServer;
use cli::{Cli, Commands};
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let log_level = match cli.log_level.as_str() {
        "debug" => Level::DEBUG,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder().with_max_level(log_level).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Some(Commands::Account(cmd)) => commands::handle_account_command(cmd).await,
        Some(Commands::Config(cmd)) => commands::handle_config_command(cmd).await,
        Some(Commands::Warmup { all, email }) => commands::handle_warmup(all, email).await,
        Some(Commands::Status) => commands::handle_status().await,
        Some(Commands::GenerateKey) => commands::handle_generate_key().await,
        Some(Commands::Serve { port }) => run_server(port).await,
        None => run_server(cli.port).await,
    }
}

async fn run_server(port: u16) -> Result<()> {
    info!("🚀 Antigravity Server starting on port {}...", port);

    let _ = antigravity_core::proxy::prometheus::init_metrics();
    info!("📊 Prometheus metrics initialized");

    let data_dir = antigravity_core::modules::account::get_data_dir()
        .map_err(|e| anyhow::anyhow!("Failed to get data directory: {}", e))?;
    let initial_app_config = antigravity_core::modules::config::load_config().unwrap_or_default();
    let initial_proxy_config = initial_app_config.proxy;

    let token_manager = Arc::new(antigravity_core::proxy::TokenManager::new(data_dir.clone()));
    match token_manager.load_accounts().await {
        Ok(count) => {
            tracing::info!("📊 Loaded {} accounts into token manager", count);
        }
        Err(e) => {
            tracing::warn!("⚠️ Could not load accounts into token manager: {}", e);
        }
    }

    token_manager.start_auto_cleanup();

    // Initialize WARP IP isolation (per-account SOCKS5 proxies)
    let warp_mapping_path = std::env::var("WARP_MAPPING_FILE").unwrap_or_else(|_| {
        antigravity_core::proxy::warp_isolation::DEFAULT_WARP_MAPPING_PATH.to_string()
    });

    let warp_manager = Arc::new(
        antigravity_core::proxy::warp_isolation::WarpIsolationManager::with_path(
            &warp_mapping_path,
        ),
    );

    match warp_manager.load_mappings().await {
        Ok(count) if count > 0 => {
            tracing::info!(
                "🔐 WARP IP isolation enabled: {} accounts mapped to SOCKS5 proxies",
                count
            );
        }
        Ok(_) => {
            tracing::info!(
                "ℹ️ WARP IP isolation: no mappings found (direct connections will be used)"
            );
        }
        Err(e) => {
            tracing::warn!("⚠️ WARP IP isolation disabled: {}", e);
        }
    }

    let monitor = Arc::new(antigravity_core::proxy::ProxyMonitor::new());

    // Create AxumServer for hot reload capabilities (without starting listener)
    let server_config = antigravity_core::proxy::server::ServerStartConfig {
        host: "127.0.0.1".to_string(),
        port,
        token_manager: token_manager.clone(),
        custom_mapping: initial_proxy_config.custom_mapping.clone(),
        upstream_proxy: initial_proxy_config.upstream_proxy.clone(),
        security_config: antigravity_core::proxy::ProxySecurityConfig::from_proxy_config(
            &initial_proxy_config,
        ),
        zai: initial_proxy_config.zai.clone(),
        monitor: monitor.clone(),
        experimental: initial_proxy_config.experimental.clone(),
        adaptive_limits: Arc::new(antigravity_core::proxy::AdaptiveLimitManager::new(
            0.85,
            antigravity_core::proxy::AIMDController::default(),
        )),
        health_monitor: antigravity_core::proxy::HealthMonitor::new(),
        circuit_breaker: Arc::new(antigravity_core::proxy::CircuitBreakerManager::new()),
    };

    let axum_server = Arc::new(AxumServer::new(server_config));

    let state = AppState::new_with_components(
        token_manager.clone(),
        monitor.clone(),
        initial_proxy_config.clone(),
        axum_server.clone(),
    )
    .await?;

    info!("✅ Application state initialized");
    info!("📊 {} accounts loaded", state.get_account_count());

    scheduler::start(state.clone());

    let app = build_router(state, axum_server).await;

    // Try to get listener from systemd socket activation (zero-downtime restarts)
    let mut listenfd = ListenFd::from_env();
    let listener = if let Some(listener) = listenfd.take_tcp_listener(0)? {
        info!("🔌 Using systemd socket activation (fd=3)");
        listener.set_nonblocking(true)?;
        tokio::net::TcpListener::from_std(listener)?
    } else {
        // Fallback: bind ourselves (for local development)
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        info!("🔌 Binding socket ourselves (no systemd activation)");
        tokio::net::TcpListener::bind(addr).await?
    };

    let local_addr = listener.local_addr()?;
    info!("🌐 Server listening on http://{}", local_addr);
    info!(
        "📊 WebUI available at http://localhost:{}/",
        local_addr.port()
    );
    info!(
        "🔌 API available at http://localhost:{}/api/",
        local_addr.port()
    );
    info!(
        "🔀 Proxy endpoints at http://localhost:{}/v1/",
        local_addr.port()
    );

    axum::serve(listener, app).await?;

    Ok(())
}

async fn build_router(state: AppState, _axum_server: Arc<AxumServer>) -> Router {
    // Get proxy router from state (uses shared Arc for hot-reload)
    let proxy_router = state.build_proxy_router();

    // Static files for WebUI (Leptos dist)
    let static_dir =
        std::env::var("ANTIGRAVITY_STATIC_DIR").unwrap_or_else(|_| "./src-leptos/dist".to_string());

    // API router with AppState
    let api_routes = Router::new()
        .nest("/api", api::router())
        .route("/health", get(health_check))
        .route("/healthz", get(health_check))
        .with_state(state);

    // SPA fallback: when a file is not found, serve index.html
    // This is the standard pattern for all SPA frameworks (React, Vue, Angular, Leptos, etc.)
    // Direct URL access to /monitor, /accounts, /proxy, /settings will serve index.html
    // and let Leptos Router handle the client-side routing
    let index_path = format!("{}/index.html", static_dir);
    let spa_service = ServeDir::new(&static_dir)
        .append_index_html_on_directories(true)
        .not_found_service(ServeFile::new(&index_path));

    // Combine: API routes + Proxy routes + SPA fallback
    api_routes
        .merge(proxy_router)
        .fallback_service(spa_service)
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}

async fn health_check() -> impl IntoResponse {
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({"status": "ok"})),
    )
}
