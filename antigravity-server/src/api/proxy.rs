//! Proxy status and management handlers

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::Serialize;

use antigravity_core::modules::config as core_config;

use crate::state::AppState;

#[derive(Serialize)]
pub struct ProxyStatusResponse {
    pub running: bool,
    pub port: u16,
    pub base_url: String,
    pub active_accounts: usize,
}

pub async fn get_proxy_status(State(state): State<AppState>) -> Json<ProxyStatusResponse> {
    let port = state.get_bound_port();
    let bind_addr = state.get_proxy_bind_address().await;

    Json(ProxyStatusResponse {
        running: true,
        port,
        base_url: format!("http://{}:{}", bind_addr, port),
        active_accounts: state.get_token_manager_count(),
    })
}

#[derive(Serialize)]
pub struct GenerateApiKeyResponse {
    pub api_key: String,
}

pub async fn generate_api_key(
    State(state): State<AppState>,
) -> Result<Json<GenerateApiKeyResponse>, (StatusCode, String)> {
    use rand::Rng;

    let key: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let api_key = format!("sk-{}", key);

    let api_key_clone = api_key.clone();
    tokio::task::spawn_blocking(move || {
        core_config::update_config(|config| {
            config.proxy.api_key.clone_from(&api_key_clone);
        })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}")))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state.hot_reload_proxy_config().await;

    Ok(Json(GenerateApiKeyResponse { api_key }))
}

pub async fn clear_session_bindings(State(state): State<AppState>) -> Json<bool> {
    state.clear_session_bindings();
    Json(true)
}

#[derive(Serialize)]
pub struct ReloadAccountsResponse {
    pub count: usize,
}

pub async fn reload_accounts(
    State(state): State<AppState>,
) -> Result<Json<ReloadAccountsResponse>, (StatusCode, String)> {
    match state.reload_accounts().await {
        Ok(count) => Ok(Json(ReloadAccountsResponse { count })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

pub async fn clear_all_rate_limits(State(state): State<AppState>) -> StatusCode {
    state.clear_all_rate_limits();
    tracing::info!("[API] Cleared all rate limit records");
    StatusCode::OK
}

pub async fn clear_rate_limit(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> StatusCode {
    if state.clear_rate_limit(&account_id) {
        tracing::info!("[API] Cleared rate limit for account {}", account_id);
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
