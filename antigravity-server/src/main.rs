//! Antigravity Server - Headless Daemon
//!
//! A pure Rust HTTP server that:
//! - Runs the proxy logic (account rotation, API forwarding)
//! - Serves the Leptos WebUI as static files
//! - Provides a REST API for CLI and UI control
//!
//! Access via: http://localhost:8045

use anyhow::Result;
use axum::{
    Router,
    routing::get,
    response::{Html, IntoResponse},
    http::StatusCode,
};
use tower_http::{
    services::ServeDir,
    cors::{CorsLayer, Any},
    trace::TraceLayer,
};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use std::net::SocketAddr;

mod api;
mod state;

use state::AppState;

const DEFAULT_PORT: u16 = 8045;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("🚀 Antigravity Server starting...");

    // Initialize application state
    let state = AppState::new().await?;
    info!("✅ Application state initialized");

    // Build the router
    let app = build_router(state);

    // Start server
    let addr = SocketAddr::from(([127, 0, 0, 1], DEFAULT_PORT));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    
    info!("🌐 Server listening on http://{}", addr);
    info!("📊 WebUI available at http://localhost:{}/", DEFAULT_PORT);
    info!("🔌 API available at http://localhost:{}/api/", DEFAULT_PORT);

    axum::serve(listener, app).await?;

    Ok(())
}

fn build_router(state: AppState) -> Router {
    // Static files for WebUI (Leptos dist)
    // In production, this would be embedded or point to a specific path
    let static_dir = std::env::var("ANTIGRAVITY_STATIC_DIR")
        .unwrap_or_else(|_| "./src-leptos/dist".to_string());

    Router::new()
        // API routes
        .nest("/api", api::router())
        // Health check
        .route("/health", get(health_check))
        // Serve static files (Leptos WebUI)
        .fallback_service(ServeDir::new(&static_dir).append_index_html_on_directories(true))
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}
