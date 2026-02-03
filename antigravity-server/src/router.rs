use axum::{
    extract::DefaultBodyLimit, http::StatusCode, response::IntoResponse, routing::get, Router,
};
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::api;
use crate::state::AppState;
use antigravity_core::proxy::server::AxumServer;

pub async fn build_router(state: AppState, _axum_server: Arc<AxumServer>) -> Router {
    let proxy_router = state.build_proxy_router();

    let static_dir =
        std::env::var("ANTIGRAVITY_STATIC_DIR").unwrap_or_else(|_| "./src-leptos/dist".to_string());

    let api_routes = Router::new()
        .nest("/api", api::router())
        .route("/health", get(health_check))
        .route("/healthz", get(health_check))
        .route("/version", get(version_info))
        .with_state(state);

    let index_path = format!("{}/index.html", static_dir);
    let spa_service = ServeDir::new(&static_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(&index_path));

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

async fn version_info() -> impl IntoResponse {
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "version": option_env!("GIT_VERSION").unwrap_or("dev"),
            "build_time": option_env!("BUILD_TIME").unwrap_or("unknown"),
            "cargo_version": env!("CARGO_PKG_VERSION"),
        })),
    )
}
