//! API Routes
//!
//! REST API endpoints that mirror the Tauri IPC commands.

use axum::{
    Router,
    routing::{get, post},
    extract::State,
    response::Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::state::{AppState, get_model_quota};

pub fn router() -> Router<AppState> {
    Router::new()
        // Status
        .route("/status", get(get_status))
        // Accounts
        .route("/accounts", get(list_accounts))
        .route("/accounts/current", get(get_current_account))
        .route("/accounts/switch", post(switch_account))
        // Proxy (placeholders for now)
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

async fn get_status(State(state): State<AppState>) -> Json<StatusResponse> {
    let current = state.get_current_account().ok().flatten();
    
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        proxy_running: false, // TODO: integrate proxy state
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
    image_quota: Option<i32>,
    subscription_tier: Option<String>,
}

async fn list_accounts(State(state): State<AppState>) -> Result<Json<Vec<AccountInfo>>, (StatusCode, String)> {
    let current_id = state.get_current_account()
        .ok()
        .flatten()
        .map(|a| a.id);
    
    match state.list_accounts() {
        Ok(accounts) => {
            let infos: Vec<AccountInfo> = accounts.into_iter().map(|a| {
                AccountInfo {
                    id: a.id.clone(),
                    email: a.email.clone(),
                    name: a.name.clone(),
                    disabled: a.disabled,
                    is_current: current_id.as_ref() == Some(&a.id),
                    gemini_quota: get_model_quota(&a, "gemini"),
                    claude_quota: get_model_quota(&a, "claude"),
                    image_quota: get_model_quota(&a, "image"),
                    subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
                }
            }).collect();
            Ok(Json(infos))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e))
    }
}

async fn get_current_account(State(state): State<AppState>) -> Result<Json<Option<AccountInfo>>, (StatusCode, String)> {
    match state.get_current_account() {
        Ok(Some(a)) => Ok(Json(Some(AccountInfo {
            id: a.id.clone(),
            email: a.email.clone(),
            name: a.name.clone(),
            disabled: a.disabled,
            is_current: true,
            gemini_quota: get_model_quota(&a, "gemini"),
            claude_quota: get_model_quota(&a, "claude"),
            image_quota: get_model_quota(&a, "image"),
            subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
        }))),
        Ok(None) => Ok(Json(None)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e))
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
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e))
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
    let running = state.is_proxy_running().await;
    let port = state.get_proxy_port().await;
    
    Json(ProxyStatusResponse {
        running,
        port,
        base_url: format!("http://127.0.0.1:{}", port),
        active_accounts: state.get_account_count(),
    })
}

#[derive(Serialize)]
struct ProxyStartResponse {
    success: bool,
    message: String,
}

async fn start_proxy(State(state): State<AppState>) -> Result<Json<ProxyStartResponse>, (StatusCode, String)> {
    match state.start_proxy().await {
        Ok(()) => Ok(Json(ProxyStartResponse {
            success: true,
            message: "Proxy started successfully".to_string(),
        })),
        Err(e) => Ok(Json(ProxyStartResponse {
            success: false,
            message: e,
        }))
    }
}

async fn stop_proxy(State(state): State<AppState>) -> Result<Json<ProxyStartResponse>, (StatusCode, String)> {
    match state.stop_proxy().await {
        Ok(()) => Ok(Json(ProxyStartResponse {
            success: true,
            message: "Proxy stopped successfully".to_string(),
        })),
        Err(e) => Ok(Json(ProxyStartResponse {
            success: false,
            message: e,
        }))
    }
}

// ============ Config (Placeholders) ============

async fn get_config(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // TODO: Load config from antigravity-core
    Json(serde_json::json!({}))
}

async fn save_config(
    State(_state): State<AppState>,
    Json(_payload): Json<serde_json::Value>,
) -> Json<bool> {
    // TODO: Save config
    Json(false)
}
