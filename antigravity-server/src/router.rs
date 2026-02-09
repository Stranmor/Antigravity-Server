use axum::{
    extract::DefaultBodyLimit, http::StatusCode, middleware, response::IntoResponse, routing::get,
    Router,
};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::api;
use crate::state::AppState;
use antigravity_core::proxy::middleware::{admin_auth_middleware, cors::cors_layer};

pub async fn build_router(state: AppState) -> Router {
    let proxy_router = state.build_proxy_router();

    let static_dir =
        std::env::var("ANTIGRAVITY_STATIC_DIR").unwrap_or_else(|_| "./src-leptos/dist".to_string());

    // Security config for admin auth middleware
    let security_config = state.inner.security_config.clone();

    // Protected API routes (require Bearer token)
    let protected_api = Router::new()
        .nest("/api", api::router())
        .layer(middleware::from_fn_with_state(security_config, admin_auth_middleware))
        .with_state(state.clone());

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/healthz", get(health_check))
        .route("/version", get(version_info))
        .with_state(state);

    let index_path = format!("{}/index.html", static_dir);
    let spa_service = ServeDir::new(&static_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(&index_path));

    protected_api
        .merge(public_routes)
        .merge(proxy_router)
        .fallback_service(spa_service)
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer())
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, axum::Json(serde_json::json!({"status": "ok"})))
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
