//! API Routes
//!
//! REST API endpoints that mirror the Tauri IPC commands.

mod device;
mod oauth;
mod resilience;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use antigravity_core::models::AppConfig;
use antigravity_core::modules::{account, config as core_config, oauth as core_oauth};
use antigravity_types::models::TokenData;

use crate::state::{get_model_quota, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        // Status
        .route("/status", get(get_status))
        // Accounts
        .route("/accounts", get(list_accounts))
        .route("/accounts/current", get(get_current_account))
        .route("/accounts/switch", post(switch_account))
        .route("/accounts/delete", post(delete_account_handler))
        .route("/accounts/delete-batch", post(delete_accounts_handler))
        .route("/accounts/add-by-token", post(add_account_by_token))
        .route("/accounts/refresh-quota", post(refresh_account_quota))
        .route("/accounts/refresh-all-quotas", post(refresh_all_quotas))
        .route("/accounts/toggle-proxy", post(toggle_proxy_status))
        .route("/accounts/warmup", post(warmup_account))
        .route("/accounts/warmup-all", post(warmup_all_accounts))
        // OAuth (headless flow)
        .route("/oauth/url", get(oauth::get_oauth_url))
        .route("/oauth/callback", get(oauth::handle_oauth_callback))
        .route("/oauth/login", post(oauth::start_oauth_login))
        .route("/oauth/submit-code", post(oauth::submit_oauth_code))
        // Proxy
        .route("/proxy/status", get(get_proxy_status))
        .route("/proxy/generate-key", post(generate_api_key))
        .route("/proxy/clear-bindings", post(clear_session_bindings))
        .route("/proxy/rate-limits", delete(clear_all_rate_limits))
        .route("/proxy/rate-limits/:account_id", delete(clear_rate_limit))
        .route("/accounts/reload", post(reload_accounts))
        // Monitor
        .route("/monitor/requests", get(get_monitor_requests))
        .route("/monitor/stats", get(get_monitor_stats))
        .route("/monitor/clear", post(clear_monitor_logs))
        // Config
        .route("/config", get(get_config))
        .route("/config", post(save_config))
        // Config Sync (LWW Bidirectional)
        .route("/config/mapping", get(get_syncable_mapping))
        .route("/config/mapping", post(merge_remote_mapping))
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
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "Not found"})),
    )
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
    proxy_disabled: bool,
    is_current: bool,
    gemini_quota: Option<i32>,
    claude_quota: Option<i32>,
    subscription_tier: Option<String>,
    quota: Option<antigravity_types::models::QuotaData>,
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
                    proxy_disabled: a.proxy_disabled,
                    is_current: current_id.as_ref() == Some(&a.id),
                    gemini_quota: get_model_quota(&a, "gemini"),
                    claude_quota: get_model_quota(&a, "claude"),
                    subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
                    quota: a.quota.clone(),
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
            proxy_disabled: a.proxy_disabled,
            is_current: true,
            gemini_quota: get_model_quota(&a, "gemini"),
            claude_quota: get_model_quota(&a, "claude"),
            subscription_tier: a.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
            quota: a.quota.clone(),
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

// ============ Account Management ============

#[derive(Deserialize)]
struct DeleteAccountRequest {
    account_id: String,
}

async fn delete_account_handler(
    State(state): State<AppState>,
    Json(payload): Json<DeleteAccountRequest>,
) -> Result<Json<bool>, (StatusCode, String)> {
    match account::delete_account(&payload.account_id) {
        Ok(()) => {
            // Reload token manager
            let _ = state.reload_accounts().await;
            Ok(Json(true))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

#[derive(Deserialize)]
struct DeleteAccountsRequest {
    account_ids: Vec<String>,
}

async fn delete_accounts_handler(
    State(state): State<AppState>,
    Json(payload): Json<DeleteAccountsRequest>,
) -> Result<Json<bool>, (StatusCode, String)> {
    match account::delete_accounts(&payload.account_ids) {
        Ok(()) => {
            // Reload token manager
            let _ = state.reload_accounts().await;
            Ok(Json(true))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

#[derive(Deserialize)]
struct AddByTokenRequest {
    refresh_tokens: Vec<String>,
}

#[derive(Serialize)]
struct AddByTokenResponse {
    success_count: usize,
    fail_count: usize,
    accounts: Vec<AccountInfo>,
}

async fn add_account_by_token(
    State(state): State<AppState>,
    Json(payload): Json<AddByTokenRequest>,
) -> Result<Json<AddByTokenResponse>, (StatusCode, String)> {
    // Return data for sequential upsert (avoid race condition on file storage)
    struct TokenResult {
        email: String,
        name: Option<String>,
        token_data: TokenData,
    }

    let mut join_set: JoinSet<Result<TokenResult, String>> = JoinSet::new();

    for token in payload.refresh_tokens {
        join_set.spawn(async move {
            let token_response = core_oauth::refresh_access_token(&token)
                .await
                .map_err(|e| format!("Token refresh failed: {}", e))?;

            let user_info = core_oauth::get_user_info(&token_response.access_token)
                .await
                .map_err(|e| format!("User info failed: {}", e))?;

            // Use rotated refresh token if provider returned new one, else keep original
            let refresh_token = token_response
                .refresh_token
                .clone()
                .unwrap_or_else(|| token.clone());

            let token_data = TokenData::new(
                token_response.access_token,
                refresh_token,
                token_response.expires_in,
                Some(user_info.email.clone()),
                None,
                None,
            );

            Ok(TokenResult {
                name: user_info.get_display_name(),
                email: user_info.email,
                token_data,
            })
        });
    }

    let mut success_count = 0;
    let mut fail_count = 0;
    let mut added_accounts = Vec::new();
    let mut errors = Vec::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(token_result)) => {
                // Sequential upsert to avoid race condition on file storage
                match account::upsert_account(
                    token_result.email.clone(),
                    token_result.name,
                    token_result.token_data,
                ) {
                    Ok(acc) => {
                        success_count += 1;
                        added_accounts.push(AccountInfo {
                            id: acc.id.clone(),
                            email: acc.email.clone(),
                            name: acc.name.clone(),
                            disabled: acc.disabled,
                            proxy_disabled: acc.proxy_disabled,
                            is_current: false,
                            gemini_quota: get_model_quota(&acc, "gemini"),
                            claude_quota: get_model_quota(&acc, "claude"),
                            subscription_tier: acc
                                .quota
                                .as_ref()
                                .and_then(|q| q.subscription_tier.clone()),
                            quota: acc.quota.clone(),
                        });
                    }
                    Err(e) => {
                        fail_count += 1;
                        errors.push(format!("{}: {}", token_result.email, e));
                        tracing::warn!("Failed to upsert account: {}", errors.last().unwrap());
                    }
                }
            }
            Ok(Err(e)) => {
                fail_count += 1;
                errors.push(e);
                tracing::warn!("Failed to add account: {}", errors.last().unwrap());
            }
            Err(e) => {
                fail_count += 1;
                tracing::error!("Task panicked: {}", e);
            }
        }
    }

    let _ = state.reload_accounts().await;

    Ok(Json(AddByTokenResponse {
        success_count,
        fail_count,
        accounts: added_accounts,
    }))
}

#[derive(Deserialize)]
struct RefreshQuotaRequest {
    account_id: String,
}

#[derive(Serialize)]
struct QuotaResponse {
    account_id: String,
    quota: Option<antigravity_types::models::QuotaData>,
}

async fn refresh_account_quota(
    State(state): State<AppState>,
    Json(payload): Json<RefreshQuotaRequest>,
) -> Result<Json<QuotaResponse>, (StatusCode, String)> {
    // Load account
    let mut acc = match account::load_account(&payload.account_id) {
        Ok(a) => a,
        Err(e) => return Err((StatusCode::NOT_FOUND, e)),
    };

    // Fetch quota with retry
    match account::fetch_quota_with_retry(&mut acc).await {
        Ok(quota) => {
            if let Err(e) =
                account::update_account_quota_async(payload.account_id.clone(), quota.clone()).await
            {
                tracing::warn!("Failed to update quota protection: {}", e);
                let _ = account::save_account(&acc);
            }
            // Reload token manager
            let _ = state.reload_accounts().await;

            Ok(Json(QuotaResponse {
                account_id: payload.account_id,
                quota: Some(quota),
            }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn refresh_all_quotas(
    State(state): State<AppState>,
) -> Result<Json<antigravity_types::models::RefreshStats>, (StatusCode, String)> {
    let accounts = match account::list_accounts() {
        Ok(a) => a,
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    };

    let total = accounts.len();
    let mut join_set: JoinSet<Result<(String, antigravity_types::models::QuotaData), String>> =
        JoinSet::new();

    for mut acc in accounts {
        if acc.disabled {
            continue;
        }

        let account_id = acc.id.clone();
        join_set.spawn(async move {
            let quota = account::fetch_quota_with_retry(&mut acc)
                .await
                .map_err(|e| format!("{}: {}", acc.email, e))?;
            Ok((account_id, quota))
        });
    }

    let mut success = 0;
    let mut failed = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok((account_id, quota))) => {
                if let Err(e) = account::update_account_quota_async(account_id.clone(), quota).await
                {
                    tracing::warn!("Quota protection update failed for {}: {}", account_id, e);
                }
                success += 1;
            }
            Ok(Err(e)) => {
                tracing::warn!("Quota refresh failed: {}", e);
                failed += 1;
            }
            Err(e) => {
                tracing::error!("Task panicked: {}", e);
                failed += 1;
            }
        }
    }

    let _ = state.reload_accounts().await;

    Ok(Json(antigravity_types::models::RefreshStats {
        total,
        success,
        failed,
    }))
}

// ============ Toggle Proxy Status ============

#[derive(Deserialize)]
struct ToggleProxyRequest {
    account_id: String,
    enable: bool,
    reason: Option<String>,
}

#[derive(Serialize)]
struct ToggleProxyResponse {
    success: bool,
    account_id: String,
    proxy_disabled: bool,
}

async fn toggle_proxy_status(
    State(state): State<AppState>,
    Json(payload): Json<ToggleProxyRequest>,
) -> Result<Json<ToggleProxyResponse>, (StatusCode, String)> {
    // Log toggle with optional reason
    tracing::info!(
        account_id = %payload.account_id,
        enable = %payload.enable,
        reason = ?payload.reason,
        "Toggling proxy status"
    );

    // Load account
    let mut acc = match account::load_account(&payload.account_id) {
        Ok(a) => a,
        Err(e) => return Err((StatusCode::NOT_FOUND, e)),
    };

    // Toggle proxy_disabled (enable=true means proxy is NOT disabled)
    acc.proxy_disabled = !payload.enable;

    // Save account
    match account::save_account(&acc) {
        Ok(_) => {
            // Reload token manager
            let _ = state.reload_accounts().await;

            Ok(Json(ToggleProxyResponse {
                success: true,
                account_id: payload.account_id,
                proxy_disabled: acc.proxy_disabled,
            }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

// ============ Warmup ============

#[derive(Deserialize)]
struct WarmupAccountRequest {
    account_id: String,
}

#[derive(Serialize)]
struct WarmupResponse {
    success: bool,
    message: String,
}

async fn warmup_account(
    State(state): State<AppState>,
    Json(payload): Json<WarmupAccountRequest>,
) -> Result<Json<WarmupResponse>, (StatusCode, String)> {
    // Load account
    let mut acc = match account::load_account(&payload.account_id) {
        Ok(a) => a,
        Err(e) => return Err((StatusCode::NOT_FOUND, e)),
    };

    // Warmup by refreshing quota (this makes an API call which "warms up" the session)
    match account::fetch_quota_with_retry(&mut acc).await {
        Ok(_) => {
            // Use update_account_quota to properly populate protected_models
            if let Some(quota) = acc.quota.clone() {
                let _ = account::update_account_quota_async(acc.id.clone(), quota).await;
            }
            let _ = state.reload_accounts().await;

            Ok(Json(WarmupResponse {
                success: true,
                message: format!("Account {} warmed up successfully", acc.email),
            }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn warmup_all_accounts(
    State(state): State<AppState>,
) -> Result<Json<WarmupResponse>, (StatusCode, String)> {
    let accounts = match account::list_accounts() {
        Ok(a) => a,
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    };

    let total = accounts.len();

    // Return data for sequential update (avoid race condition on file storage)
    struct WarmupResult {
        account_id: String,
        quota: Option<antigravity_types::models::QuotaData>,
    }

    let mut join_set: JoinSet<Result<WarmupResult, String>> = JoinSet::new();

    for mut acc in accounts {
        if acc.disabled || acc.proxy_disabled {
            continue;
        }

        let account_id = acc.id.clone();
        let email = acc.email.clone();
        join_set.spawn(async move {
            account::fetch_quota_with_retry(&mut acc)
                .await
                .map_err(|e| format!("{}: {}", email, e))?;

            Ok(WarmupResult {
                account_id,
                quota: acc.quota,
            })
        });
    }

    let mut success = 0;
    let mut failed = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(warmup_result)) => {
                if let Some(quota) = warmup_result.quota {
                    // Sequential update to avoid race condition on file storage
                    let _ = account::update_account_quota_async(
                        warmup_result.account_id.clone(),
                        quota,
                    )
                    .await;
                }
                success += 1;
            }
            Ok(Err(e)) => {
                tracing::warn!("Warmup failed: {}", e);
                failed += 1;
            }
            Err(e) => {
                tracing::error!("Task panicked: {}", e);
                failed += 1;
            }
        }
    }

    let _ = state.reload_accounts().await;

    Ok(Json(WarmupResponse {
        success: true,
        message: format!(
            "Warmup complete: {}/{} accounts warmed up ({} failed)",
            success, total, failed
        ),
    }))
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
    let port = state.get_bound_port();
    let bind_addr = state.get_proxy_bind_address().await;

    Json(ProxyStatusResponse {
        running: true, // Always running on same port
        port,
        base_url: format!("http://{}:{}", bind_addr, port),
        active_accounts: state.get_token_manager_count(),
    })
}

#[derive(Serialize)]
struct GenerateApiKeyResponse {
    api_key: String,
}

async fn generate_api_key(
    State(state): State<AppState>,
) -> Result<Json<GenerateApiKeyResponse>, (StatusCode, String)> {
    use rand::Rng;

    let key: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let api_key = format!("sk-{}", key);

    core_config::update_config(|config| {
        config.proxy.api_key = api_key.clone();
    })
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state.hot_reload_proxy_config().await;

    Ok(Json(GenerateApiKeyResponse { api_key }))
}

async fn clear_session_bindings(State(state): State<AppState>) -> Json<bool> {
    state.clear_session_bindings();
    Json(true)
}

#[derive(Serialize)]
struct ReloadAccountsResponse {
    count: usize,
}

async fn reload_accounts(
    State(state): State<AppState>,
) -> Result<Json<ReloadAccountsResponse>, (StatusCode, String)> {
    match state.reload_accounts().await {
        Ok(count) => Ok(Json(ReloadAccountsResponse { count })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

async fn clear_all_rate_limits(State(state): State<AppState>) -> StatusCode {
    state.clear_all_rate_limits();
    tracing::info!("[API] Cleared all rate limit records");
    StatusCode::OK
}

async fn clear_rate_limit(
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

// ============ Monitor ============

#[derive(Deserialize)]
struct MonitorQuery {
    limit: Option<usize>,
}

async fn get_monitor_requests(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<MonitorQuery>,
) -> Json<Vec<antigravity_types::models::ProxyRequestLog>> {
    let logs = state.get_proxy_logs(query.limit).await;
    Json(logs)
}

async fn get_monitor_stats(
    State(state): State<AppState>,
) -> Json<antigravity_types::models::ProxyStats> {
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

async fn get_syncable_mapping(
    State(state): State<AppState>,
) -> Json<antigravity_types::SyncableMapping> {
    Json(state.get_syncable_mapping().await)
}

#[derive(Deserialize)]
struct MergeMappingRequest {
    mapping: antigravity_types::SyncableMapping,
}

#[derive(Serialize)]
struct MergeMappingResponse {
    updated_count: usize,
    total_count: usize,
}

async fn merge_remote_mapping(
    State(state): State<AppState>,
    Json(payload): Json<MergeMappingRequest>,
) -> Json<MergeMappingResponse> {
    let updated = state.merge_remote_mapping(&payload.mapping).await;
    let total = state.get_syncable_mapping().await.len();

    Json(MergeMappingResponse {
        updated_count: updated,
        total_count: total,
    })
}
