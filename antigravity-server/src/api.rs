//! API Routes
//!
//! REST API endpoints that mirror the Tauri IPC commands.

use axum::{
    Router,
    routing::{get, post},
    extract::State,
    response::Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

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
        .route("/proxy/start", post(start_proxy))
        .route("/proxy/stop", post(stop_proxy))
        // Config
        .route("/config", get(get_config))
        .route("/config", post(save_config))
}

// ============ Status ============

#[derive(Serialize)]
struct StatusResponse {
    version: String,
    proxy_running: bool,
    accounts_count: usize,
    current_account: Option<String>,
}

async fn get_status(State(_state): State<AppState>) -> Json<StatusResponse> {
    // TODO: Get real status from state
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        proxy_running: false,
        accounts_count: 0,
        current_account: None,
    })
}

// ============ Accounts ============

#[derive(Serialize)]
struct AccountInfo {
    id: String,
    email: String,
    enabled: bool,
    // TODO: Add quota info
}

async fn list_accounts(State(_state): State<AppState>) -> Json<Vec<AccountInfo>> {
    // TODO: Get from antigravity-core
    Json(vec![])
}

async fn get_current_account(State(_state): State<AppState>) -> Json<Option<AccountInfo>> {
    // TODO: Get from state
    Json(None)
}

#[derive(Deserialize)]
struct SwitchAccountRequest {
    account_id: String,
}

async fn switch_account(
    State(_state): State<AppState>,
    Json(_payload): Json<SwitchAccountRequest>,
) -> Json<bool> {
    // TODO: Implement account switching
    Json(false)
}

// ============ Proxy ============

#[derive(Serialize)]
struct ProxyStatusResponse {
    running: bool,
    port: u16,
    address: String,
}

async fn get_proxy_status(State(_state): State<AppState>) -> Json<ProxyStatusResponse> {
    // TODO: Get real proxy status
    Json(ProxyStatusResponse {
        running: false,
        port: 8045,
        address: "127.0.0.1".to_string(),
    })
}

async fn start_proxy(State(_state): State<AppState>) -> Json<bool> {
    // TODO: Start proxy
    Json(false)
}

async fn stop_proxy(State(_state): State<AppState>) -> Json<bool> {
    // TODO: Stop proxy
    Json(false)
}

// ============ Config ============

#[derive(Serialize, Deserialize)]
struct ConfigResponse {
    // TODO: Mirror AppConfig from antigravity-shared
}

async fn get_config(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // TODO: Get config
    Json(serde_json::json!({}))
}

async fn save_config(
    State(_state): State<AppState>,
    Json(_payload): Json<serde_json::Value>,
) -> Json<bool> {
    // TODO: Save config
    Json(false)
}
