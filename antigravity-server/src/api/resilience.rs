use axum::{extract::State, response::Json};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthStatusResponse {
    pub healthy_accounts: usize,
    pub disabled_accounts: usize,
    pub overall_healthy: bool,
}

pub async fn get_health_status(State(state): State<AppState>) -> Json<HealthStatusResponse> {
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
pub struct CircuitStatusResponse {
    pub circuits: std::collections::HashMap<String, String>,
}

pub async fn get_circuit_status(State(state): State<AppState>) -> Json<CircuitStatusResponse> {
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
pub struct AimdStatusResponse {
    pub tracked_accounts: usize,
    pub accounts: Vec<antigravity_core::proxy::AimdAccountStats>,
}

pub async fn get_aimd_status(State(state): State<AppState>) -> Json<AimdStatusResponse> {
    let adaptive_limits = state.adaptive_limits();
    let accounts = adaptive_limits.all_stats();

    Json(AimdStatusResponse { tracked_accounts: adaptive_limits.len(), accounts })
}

pub async fn get_metrics(
    State(state): State<AppState>,
) -> axum::response::Response<axum::body::Body> {
    use axum::http::header;
    use axum::response::IntoResponse;

    let accounts = state.list_accounts().unwrap_or_default();
    let available = accounts.iter().filter(|a| !a.disabled && !a.proxy_disabled).count();
    antigravity_core::proxy::prometheus::update_account_gauges(accounts.len(), available);
    antigravity_core::proxy::prometheus::update_uptime_gauge();
    let metrics = antigravity_core::proxy::prometheus::render_metrics();

    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")], metrics).into_response()
}
