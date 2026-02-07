//! API Routes
//!
//! REST API endpoints that mirror the Tauri IPC commands.

mod accounts;
mod config;
mod device;
mod monitor;
mod oauth;
mod proxy;
mod quota;
mod resilience;

#[cfg(test)]
mod monitor_tests;
#[cfg(test)]
mod proxy_tests;
#[cfg(test)]
mod resilience_tests;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use serde::Serialize;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Status
        .route("/status", get(get_status))
        // Accounts
        .route("/accounts", get(accounts::list_accounts))
        .route("/accounts/current", get(accounts::get_current_account))
        .route("/accounts/switch", post(accounts::switch_account))
        .route("/accounts/delete", post(accounts::delete_account_handler))
        .route(
            "/accounts/delete-batch",
            post(accounts::delete_accounts_handler),
        )
        .route(
            "/accounts/add-by-token",
            post(accounts::add_account_by_token),
        )
        .route(
            "/accounts/refresh-quota",
            post(quota::refresh_account_quota),
        )
        .route(
            "/accounts/refresh-all-quotas",
            post(quota::refresh_all_quotas),
        )
        .route("/accounts/toggle-proxy", post(quota::toggle_proxy_status))
        .route("/accounts/warmup", post(quota::warmup_account))
        .route("/accounts/warmup-all", post(quota::warmup_all_accounts))
        // OAuth (headless flow)
        .route("/oauth/url", get(oauth::get_oauth_url))
        .route("/oauth/callback", get(oauth::handle_oauth_callback))
        .route("/oauth/login", post(oauth::start_oauth_login))
        .route("/oauth/submit-code", post(oauth::submit_oauth_code))
        // Proxy
        .route("/proxy/status", get(proxy::get_proxy_status))
        .route("/proxy/generate-key", post(proxy::generate_api_key))
        .route("/proxy/clear-bindings", post(proxy::clear_session_bindings))
        .route("/proxy/rate-limits", delete(proxy::clear_all_rate_limits))
        .route(
            "/proxy/rate-limits/:account_id",
            delete(proxy::clear_rate_limit),
        )
        .route("/accounts/reload", post(proxy::reload_accounts))
        // Monitor
        .route("/monitor/requests", get(monitor::get_monitor_requests))
        .route("/monitor/stats", get(monitor::get_monitor_stats))
        .route("/monitor/clear", post(monitor::clear_monitor_logs))
        .route("/monitor/token-stats", get(monitor::get_token_usage_stats))
        // Config
        .route("/config", get(config::get_config))
        .route("/config", post(config::save_config))
        // Config Sync (LWW Bidirectional)
        .route("/config/mapping", get(config::get_syncable_mapping))
        .route("/config/mapping", post(config::merge_remote_mapping))
        // Device Fingerprint
        .route("/device/profile", get(device::get_device_profile))
        .route("/device/profile", post(device::create_device_profile))
        .route("/device/backup", post(device::backup_device_storage))
        .route("/device/baseline", get(device::get_device_baseline))
        // Resilience (AIMD, Circuit Breaker, Health)
        .route("/resilience/health", get(resilience::get_health_status))
        .route("/resilience/circuits", get(resilience::get_circuit_status))
        .route("/resilience/aimd", get(resilience::get_aimd_status))
        // Prometheus metrics
        .route("/metrics", get(resilience::get_metrics))
        // API fallback: return 404 for unknown API endpoints
        .fallback(api_not_found)
}

async fn api_not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Not found"})))
}

#[derive(Serialize)]
struct StatusResponse {
    version: String,
    proxy_running: bool,
    accounts_count: usize,
    current_account: Option<String>,
}

async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let current = state.get_current_account().ok().flatten();

    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        proxy_running: true,
        accounts_count: state.get_account_count(),
        current_account: current.map(|a| a.email),
    })
}
