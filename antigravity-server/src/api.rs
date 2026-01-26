//! API Routes
//!
//! REST API endpoints that mirror the Tauri IPC commands.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use antigravity_core::models::AppConfig;
use antigravity_core::modules::{account, config as core_config, oauth};
use antigravity_shared::models::TokenData;

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
        .route("/oauth/url", get(get_oauth_url))
        .route("/oauth/callback", get(handle_oauth_callback))
        .route("/oauth/login", post(start_oauth_login))
        // Proxy
        .route("/proxy/status", get(get_proxy_status))
        .route("/proxy/generate-key", post(generate_api_key))
        .route("/proxy/clear-bindings", post(clear_session_bindings))
        .route("/accounts/reload", post(reload_accounts))
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
    quota: Option<antigravity_shared::models::QuotaData>,
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
    let mut join_set: JoinSet<Result<antigravity_shared::models::Account, String>> = JoinSet::new();

    for token in payload.refresh_tokens {
        join_set.spawn(async move {
            let token_response = oauth::refresh_access_token(&token)
                .await
                .map_err(|e| format!("Token refresh failed: {}", e))?;

            let user_info = oauth::get_user_info(&token_response.access_token)
                .await
                .map_err(|e| format!("User info failed: {}", e))?;

            let token_data = TokenData::new(
                token_response.access_token,
                token.clone(),
                token_response.expires_in,
                Some(user_info.email.clone()),
                None,
                None,
            );

            account::upsert_account(
                user_info.email.clone(),
                user_info.get_display_name(),
                token_data,
            )
            .map_err(|e| format!("Upsert failed: {}", e))
        });
    }

    let mut success_count = 0;
    let mut fail_count = 0;
    let mut added_accounts = Vec::new();
    let mut errors = Vec::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(acc)) => {
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
                    subscription_tier: acc.quota.as_ref().and_then(|q| q.subscription_tier.clone()),
                    quota: acc.quota.clone(),
                });
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
    quota: Option<antigravity_shared::models::QuotaData>,
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
            if let Err(e) = account::update_account_quota(&payload.account_id, quota.clone()) {
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
) -> Result<Json<antigravity_shared::models::RefreshStats>, (StatusCode, String)> {
    let accounts = match account::list_accounts() {
        Ok(a) => a,
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    };

    let total = accounts.len();
    let mut join_set: JoinSet<Result<(String, antigravity_shared::models::QuotaData), String>> =
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
                if let Err(e) = account::update_account_quota(&account_id, quota) {
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

    Ok(Json(antigravity_shared::models::RefreshStats {
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
                let _ = account::update_account_quota(&acc.id, quota);
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
    let mut join_set: JoinSet<Result<String, String>> = JoinSet::new();

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

            if let Some(quota) = acc.quota.clone() {
                let _ = account::update_account_quota(&account_id, quota);
            }
            Ok(email)
        });
    }

    let mut success = 0;
    let mut failed = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(_)) => success += 1,
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
    let port = state.get_proxy_port().await;
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
struct AimdStatusResponse {
    tracked_accounts: usize,
    accounts: Vec<antigravity_core::proxy::AimdAccountStats>,
}

async fn get_aimd_status(State(state): State<AppState>) -> Json<AimdStatusResponse> {
    let adaptive_limits = state.adaptive_limits();
    let accounts = adaptive_limits.all_stats();

    Json(AimdStatusResponse {
        tracked_accounts: adaptive_limits.len(),
        accounts,
    })
}

// ============ Prometheus Metrics ============

/// Get Prometheus metrics in text format.
/// Returns metrics compatible with Prometheus/OpenMetrics format.
async fn get_metrics(State(state): State<AppState>) -> axum::response::Response<axum::body::Body> {
    use axum::http::header;
    use axum::response::IntoResponse;

    // Update account gauges before rendering
    let accounts = state.list_accounts().unwrap_or_default();
    let available = accounts
        .iter()
        .filter(|a| !a.disabled && !a.proxy_disabled)
        .count();
    antigravity_core::proxy::prometheus::update_account_gauges(accounts.len(), available);

    // Update uptime
    antigravity_core::proxy::prometheus::update_uptime_gauge();

    // Render metrics
    let metrics = antigravity_core::proxy::prometheus::render_metrics();

    // Return with proper content type for Prometheus
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        metrics,
    )
        .into_response()
}

// ============ OAuth (Headless Flow) ============

#[derive(Serialize)]
struct OAuthUrlResponse {
    url: String,
    redirect_uri: String,
    state: String,
}

async fn get_oauth_url(State(state): State<AppState>) -> Json<OAuthUrlResponse> {
    let port = state.get_proxy_port().await;
    let redirect_uri = get_oauth_redirect_uri_with_port(port);
    let oauth_state = state.generate_oauth_state();
    let url = oauth::get_auth_url_with_state(&redirect_uri, &oauth_state);

    Json(OAuthUrlResponse {
        url,
        redirect_uri,
        state: oauth_state,
    })
}

fn get_oauth_redirect_uri_with_port(port: u16) -> String {
    if let Ok(host) = std::env::var("ANTIGRAVITY_OAUTH_HOST") {
        format!("{}/api/oauth/callback", host)
    } else {
        format!("http://127.0.0.1:{}/api/oauth/callback", port)
    }
}

#[derive(Deserialize)]
struct OAuthCallbackQuery {
    code: Option<String>,
    error: Option<String>,
    state: Option<String>,
}

/// OAuth callback handler.
/// Google redirects here after user authorizes the app.
async fn handle_oauth_callback(
    State(app_state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<OAuthCallbackQuery>,
) -> impl IntoResponse {
    use axum::response::Html;

    // Validate CSRF state token
    let _oauth_state = match query.state {
        Some(s) if app_state.validate_oauth_state(&s) => s,
        Some(_) => {
            return Html(
                r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Invalid State Token</h1>
                    <p>CSRF validation failed. Please try again.</p>
                </body>
                </html>"#
                    .to_string(),
            )
            .into_response();
        }
        None => {
            return Html(
                r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Missing State Token</h1>
                    <p>No state parameter received. Please try again.</p>
                </body>
                </html>"#
                    .to_string(),
            )
            .into_response();
        }
    };

    // Check for error
    if let Some(error) = query.error {
        return Html(format!(
            r#"<!DOCTYPE html>
            <html>
            <head><meta charset="utf-8"><title>OAuth Error</title></head>
            <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                <h1 style="color: red;">❌ Authorization Failed</h1>
                <p>Error: {}</p>
                <p>Please close this window and try again.</p>
            </body>
            </html>"#,
            error
        ))
        .into_response();
    }

    // Check for code
    let Some(code) = query.code else {
        return Html(
            r#"<!DOCTYPE html>
            <html>
            <head><meta charset="utf-8"><title>OAuth Error</title></head>
            <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                <h1 style="color: red;">❌ Missing Authorization Code</h1>
                <p>No authorization code received.</p>
            </body>
            </html>"#
                .to_string(),
        )
        .into_response();
    };

    let port = app_state.get_proxy_port().await;
    let redirect_uri = get_oauth_redirect_uri_with_port(port);

    // Exchange code for tokens
    let token_res = match oauth::exchange_code(&code, &redirect_uri).await {
        Ok(t) => t,
        Err(e) => {
            return Html(format!(
                r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Token Exchange Failed</h1>
                    <p>Error: {}</p>
                </body>
                </html>"#,
                e
            ))
            .into_response();
        }
    };

    // Check refresh token
    let Some(refresh_token) = token_res.refresh_token else {
        return Html(r#"<!DOCTYPE html>
            <html>
            <head><meta charset="utf-8"><title>OAuth Error</title></head>
            <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                <h1 style="color: orange;">⚠️ No Refresh Token</h1>
                <p>Google didn't return a refresh token.</p>
                <p>This usually happens if you've authorized this app before.</p>
                <p><strong>Solution:</strong></p>
                <ol style="text-align: left; display: inline-block;">
                    <li>Go to <a href="https://myaccount.google.com/permissions" target="_blank">Google Account Permissions</a></li>
                    <li>Find and revoke "Antigravity Tools"</li>
                    <li>Try authorization again</li>
                </ol>
            </body>
            </html>"#.to_string()).into_response();
    };

    // Get user info
    let user_info = match oauth::get_user_info(&token_res.access_token).await {
        Ok(u) => u,
        Err(e) => {
            return Html(format!(
                r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Failed to Get User Info</h1>
                    <p>Error: {}</p>
                </body>
                </html>"#,
                e
            ))
            .into_response();
        }
    };

    // Create TokenData
    let token_data = TokenData::new(
        token_res.access_token,
        refresh_token,
        token_res.expires_in,
        Some(user_info.email.clone()),
        None, // project_id - will be fetched lazily
        None, // session_id
    );

    // Upsert account
    match account::upsert_account(
        user_info.email.clone(),
        user_info.get_display_name(),
        token_data,
    ) {
        Ok(acc) => {
            let _ = app_state.reload_accounts().await;

            Html(format!(
                r#"<!DOCTYPE html>
                <html>
                <head>
                    <meta charset="utf-8">
                    <title>Authorization Successful</title>
                </head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: green;">✅ Authorization Successful!</h1>
                    <p>Account added: <strong>{}</strong></p>
                    <p>You can close this window and return to the app.</p>
                    <script>setTimeout(function() {{ window.close(); }}, 3000);</script>
                </body>
                </html>"#,
                acc.email
            ))
            .into_response()
        }
        Err(e) => Html(format!(
            r#"<!DOCTYPE html>
                <html>
                <head><meta charset="utf-8"><title>OAuth Error</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1 style="color: red;">❌ Failed to Save Account</h1>
                    <p>Error: {}</p>
                </body>
                </html>"#,
            e
        ))
        .into_response(),
    }
}

/// POST endpoint for frontend to initiate OAuth login.
/// Returns the OAuth URL for the frontend to redirect/open.
#[derive(Serialize)]
struct OAuthLoginResponse {
    url: String,
    message: String,
    state: String,
}

async fn start_oauth_login(State(state): State<AppState>) -> Json<OAuthLoginResponse> {
    let port = state.get_proxy_port().await;
    let redirect_uri = get_oauth_redirect_uri_with_port(port);
    let oauth_state = state.generate_oauth_state();
    let url = oauth::get_auth_url_with_state(&redirect_uri, &oauth_state);

    Json(OAuthLoginResponse {
        url,
        message: "Open this URL in your browser to authorize".to_string(),
        state: oauth_state,
    })
}
