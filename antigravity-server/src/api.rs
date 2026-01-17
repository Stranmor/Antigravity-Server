//! API Routes
//!
//! REST API endpoints that mirror the Tauri IPC commands.

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use antigravity_core::models::AppConfig;
use antigravity_core::modules::config as core_config;

use crate::state::{get_model_quota, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        // Status
        .route("/status", get(get_status))
        // Accounts
        .route("/accounts", get(list_accounts))
        .route("/accounts/current", get(get_current_account))
        .route("/accounts/switch", post(switch_account))
        // Proxy
        .route("/proxy/status", get(get_proxy_status))
        // Monitor
        .route("/monitor/requests", get(get_monitor_requests))
        .route("/monitor/stats", get(get_monitor_stats))
        .route("/monitor/clear", post(clear_monitor_logs))
        // Config
        .route("/config", get(get_config))
        .route("/config", post(save_config))
        // Resilience (AIMD, Circuit Breaker, Health)
        .route("/resilience/health", get(get_health_status))
        .route("/resilience/circuits", get(get_circuit_status))
        .route("/resilience/aimd", get(get_aimd_status))
        // Prometheus metrics
        .route("/metrics", get(get_metrics))
}

// ============ Status ============

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
        proxy_running: true, // Proxy is always running on same port now
        accounts_count: state.get_account_count(),
        current_account: current.map(|a| a.email),
    })
}

// ============ Accounts ============

#[derive(Serialize)]
struct AccountInfo {
    id: String,
    email: String,
    name: Option<String>,
    disabled: bool,
    is_current: bool,
    gemini_quota: Option<i32>,
    claude_quota: Option<i32>,
    subscription_tier: Option<String>,
}

async fn list_accounts(
    State(state): State<AppState>,
) -> Result<Json<Vec<AccountInfo>>, (StatusCode, String)> {
    let current_id = state.get_current_account().ok().flatten().map(|a| a.id);

    match state.list_accounts() {
        Ok(accounts) => {
            let infos: Vec<AccountInfo> = accounts
                .into_iter()
                .map(|a| AccountInfo {
                    id: a.id.clone(),
                    email: a.email.clone(),
                    name: a.name.clone(),
                    disabled: a.disabled,
                    is_current: current_id.as_ref() == Some(&a.id),
                    gemini_quota: get_model_quota(&a, "gemini"),
                    claude_quota: get_model_quota(&a, "claude"),
                    subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
                })
                .collect();
            Ok(Json(infos))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

async fn get_current_account(
    State(state): State<AppState>,
) -> Result<Json<Option<AccountInfo>>, (StatusCode, String)> {
    match state.get_current_account() {
        Ok(Some(a)) => Ok(Json(Some(AccountInfo {
            id: a.id.clone(),
            email: a.email.clone(),
            name: a.name.clone(),
            disabled: a.disabled,
            is_current: true,
            gemini_quota: get_model_quota(&a, "gemini"),
            claude_quota: get_model_quota(&a, "claude"),
            subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
        }))),
        Ok(None) => Ok(Json(None)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

#[derive(Deserialize)]
struct SwitchAccountRequest {
    account_id: String,
}

async fn switch_account(
    State(state): State<AppState>,
    Json(payload): Json<SwitchAccountRequest>,
) -> Result<Json<bool>, (StatusCode, String)> {
    match state.switch_account(&payload.account_id).await {
        Ok(()) => Ok(Json(true)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

// ============ Proxy ============

#[derive(Serialize)]
struct ProxyStatusResponse {
    running: bool,
    port: u16,
    base_url: String,
    active_accounts: usize,
}

async fn get_proxy_status(State(state): State<AppState>) -> Json<ProxyStatusResponse> {
    let port = state.get_proxy_port().await;

    Json(ProxyStatusResponse {
        running: true, // Always running on same port
        port,
        base_url: format!("http://127.0.0.1:{}", port),
        active_accounts: state.get_token_manager_count(),
    })
}

// ============ Monitor ============

#[derive(Deserialize)]
struct MonitorQuery {
    limit: Option<usize>,
}

async fn get_monitor_requests(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<MonitorQuery>,
) -> Json<Vec<antigravity_shared::models::ProxyRequestLog>> {
    let logs = state.get_proxy_logs(query.limit).await;
    Json(logs)
}

async fn get_monitor_stats(
    State(state): State<AppState>,
) -> Json<antigravity_shared::models::ProxyStats> {
    let stats = state.get_proxy_stats().await;
    Json(stats)
}

async fn clear_monitor_logs(State(state): State<AppState>) -> Json<bool> {
    state.clear_proxy_logs().await;
    Json(true)
}

// ============ Config (Placeholders) ============

async fn get_config(
    State(_state): State<AppState>,
) -> Result<Json<AppConfig>, (StatusCode, String)> {
    match core_config::load_config() {
        Ok(config) => Ok(Json(config)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

async fn save_config(
    State(state): State<AppState>,
    Json(payload): Json<AppConfig>,
) -> Result<Json<bool>, (StatusCode, String)> {
    match core_config::save_config(&payload) {
        Ok(_) => {
            state.hot_reload_proxy_config().await;
            Ok(Json(true))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

// ============ Resilience (AIMD/Circuit Breaker/Health) ============

#[derive(Serialize)]
struct HealthStatusResponse {
    healthy_accounts: usize,
    disabled_accounts: usize,
    overall_healthy: bool,
}

async fn get_health_status(State(state): State<AppState>) -> Json<HealthStatusResponse> {
    let health_monitor = state.health_monitor();

    let healthy = health_monitor.healthy_count();
    let disabled = health_monitor.disabled_count();

    Json(HealthStatusResponse {
        healthy_accounts: healthy,
        disabled_accounts: disabled,
        overall_healthy: healthy > 0,
    })
}

#[derive(Serialize)]
struct CircuitStatusResponse {
    circuits: std::collections::HashMap<String, String>,
}

async fn get_circuit_status(State(state): State<AppState>) -> Json<CircuitStatusResponse> {
    let circuit_breaker = state.circuit_breaker();

    let mut circuits = std::collections::HashMap::new();
    for provider in ["anthropic", "google", "openai"] {
        let state_str = match circuit_breaker.get_state(provider) {
            antigravity_core::proxy::CircuitState::Closed => "closed",
            antigravity_core::proxy::CircuitState::Open => "open",
            antigravity_core::proxy::CircuitState::HalfOpen => "half_open",
        };
        circuits.insert(provider.to_string(), state_str.to_string());
    }

    Json(CircuitStatusResponse { circuits })
}

#[derive(Serialize)]
struct AimdAccountStats {
    account_id: String,
    confirmed_limit: u64,
    ceiling: u64,
    requests_this_minute: u64,
}

#[derive(Serialize)]
struct AimdStatusResponse {
    tracked_accounts: usize,
    accounts: Vec<AimdAccountStats>,
}

async fn get_aimd_status(State(state): State<AppState>) -> Json<AimdStatusResponse> {
    let adaptive_limits = state.adaptive_limits();

    let all_persisted = adaptive_limits.all_for_persistence();
    let accounts: Vec<AimdAccountStats> = all_persisted
        .into_iter()
        .map(
            |(account_id, confirmed_limit, ceiling, requests)| AimdAccountStats {
                account_id,
                confirmed_limit,
                ceiling,
                requests_this_minute: requests,
            },
        )
        .collect();

    Json(AimdStatusResponse {
        tracked_accounts: adaptive_limits.len(),
        accounts,
    })
}

// ============ Prometheus Metrics ============

/// Get Prometheus metrics in text format.
/// Returns metrics compatible with Prometheus/OpenMetrics format.
async fn get_metrics(
    State(state): State<AppState>,
) -> axum::response::Response<axum::body::Body> {
    use axum::http::header;
    use axum::response::IntoResponse;

    // Update account gauges before rendering
    let accounts = state.list_accounts().unwrap_or_default();
    let available = accounts.iter().filter(|a| !a.disabled && !a.proxy_disabled).count();
    antigravity_core::proxy::prometheus::update_account_gauges(accounts.len(), available);

    // Update uptime
    antigravity_core::proxy::prometheus::update_uptime_gauge();

    // Render metrics
    let metrics = antigravity_core::proxy::prometheus::render_metrics();

    // Return with proper content type for Prometheus
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        metrics,
    )
        .into_response()
}
