//! Per-account proxy management API handlers.

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};

use antigravity_core::modules::account;

use super::proxy_health::{check_proxy_health, validate_proxy_url};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SetProxyRequest {
    pub account_id: String,
    pub proxy_url: String,
}

#[derive(Serialize)]
pub struct SetProxyResponse {
    pub success: bool,
    pub exit_ip: String,
}

pub async fn set_proxy_handler(
    State(state): State<AppState>,
    Json(payload): Json<SetProxyRequest>,
) -> Result<Json<SetProxyResponse>, (StatusCode, String)> {
    validate_proxy_url(&payload.proxy_url).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let exit_ip = check_proxy_health(&payload.proxy_url)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Proxy health check failed: {e}")))?;

    // Validate account exists BEFORE persisting (avoids inconsistent state if email lookup fails)
    let email = get_account_email(&state, &payload.account_id).await?;

    persist_proxy_url(&state, &payload.account_id, Some(&payload.proxy_url)).await?;

    drop(state.reload_accounts().await);

    // Record timestamp for LWW sync
    state.update_proxy_assignment(&email, Some(payload.proxy_url)).await;

    Ok(Json(SetProxyResponse { success: true, exit_ip }))
}

#[derive(Deserialize)]
pub struct RemoveProxyRequest {
    pub account_id: String,
}

#[derive(Serialize)]
pub struct RemoveProxyResponse {
    pub success: bool,
}

pub async fn remove_proxy_handler(
    State(state): State<AppState>,
    Json(payload): Json<RemoveProxyRequest>,
) -> Result<Json<RemoveProxyResponse>, (StatusCode, String)> {
    let email = get_account_email(&state, &payload.account_id).await?;

    persist_proxy_url(&state, &payload.account_id, None).await?;

    drop(state.reload_accounts().await);

    state.update_proxy_assignment(&email, None).await;

    Ok(Json(RemoveProxyResponse { success: true }))
}

#[derive(Deserialize)]
pub struct BulkProxyAssignment {
    pub account_id: String,
    pub proxy_url: String,
}

#[derive(Deserialize)]
pub struct SetProxyBulkRequest {
    pub assignments: Vec<BulkProxyAssignment>,
}

#[derive(Serialize)]
pub struct BulkProxyResult {
    pub account_id: String,
    pub success: bool,
    pub exit_ip: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct SetProxyBulkResponse {
    pub results: Vec<BulkProxyResult>,
}

const MAX_BULK_ASSIGNMENTS: usize = 100;

pub async fn set_proxy_bulk_handler(
    State(state): State<AppState>,
    Json(payload): Json<SetProxyBulkRequest>,
) -> Result<Json<SetProxyBulkResponse>, (StatusCode, String)> {
    if payload.assignments.len() > MAX_BULK_ASSIGNMENTS {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Too many assignments: {} (max {MAX_BULK_ASSIGNMENTS})",
                payload.assignments.len()
            ),
        ));
    }

    let unique_urls: std::collections::HashSet<&str> =
        payload.assignments.iter().map(|a| a.proxy_url.as_str()).collect();

    let mut health_results: std::collections::HashMap<String, Result<String, String>> =
        std::collections::HashMap::new();

    for url in unique_urls {
        let result = match validate_proxy_url(url) {
            Ok(()) => check_proxy_health(url).await,
            Err(e) => Err(e),
        };
        health_results.insert(url.to_string(), result);
    }

    let mut results = Vec::with_capacity(payload.assignments.len());

    for assignment in &payload.assignments {
        let health = health_results
            .get(&assignment.proxy_url)
            .cloned()
            .unwrap_or_else(|| Err("Missing health check result".to_string()));

        match health {
            Ok(exit_ip) => {
                match persist_proxy_url(&state, &assignment.account_id, Some(&assignment.proxy_url))
                    .await
                {
                    Ok(()) => {
                        match get_account_email(&state, &assignment.account_id).await {
                            Ok(email) => {
                                state
                                    .update_proxy_assignment(
                                        &email,
                                        Some(assignment.proxy_url.clone()),
                                    )
                                    .await;
                            },
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to resolve email for account {} during bulk proxy set: {}",
                                    assignment.account_id,
                                    e.1
                                );
                            },
                        }
                        results.push(BulkProxyResult {
                            account_id: assignment.account_id.clone(),
                            success: true,
                            exit_ip: Some(exit_ip),
                            error: None,
                        });
                    },
                    Err((_, e)) => {
                        results.push(BulkProxyResult {
                            account_id: assignment.account_id.clone(),
                            success: false,
                            exit_ip: None,
                            error: Some(e),
                        });
                    },
                }
            },
            Err(e) => {
                results.push(BulkProxyResult {
                    account_id: assignment.account_id.clone(),
                    success: false,
                    exit_ip: None,
                    error: Some(format!("Health check failed: {e}")),
                });
            },
        }
    }

    drop(state.reload_accounts().await);

    Ok(Json(SetProxyBulkResponse { results }))
}

#[derive(Serialize)]
pub struct AccountProxyStatus {
    pub account_id: String,
    pub email: String,
    pub proxy_url: Option<String>,
    pub disabled: bool,
}

#[derive(Serialize)]
pub struct ProxyStatusResponse {
    pub enforce_proxy: bool,
    pub accounts: Vec<AccountProxyStatus>,
}

pub async fn proxy_status_handler(
    State(state): State<AppState>,
) -> Result<Json<ProxyStatusResponse>, (StatusCode, String)> {
    let accounts =
        state.list_accounts().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let enforce_proxy = state.enforce_proxy().await;

    let account_statuses: Vec<AccountProxyStatus> = accounts
        .into_iter()
        .map(|a| AccountProxyStatus {
            account_id: a.id,
            email: a.email,
            proxy_url: a.proxy_url,
            disabled: a.disabled,
        })
        .collect();

    Ok(Json(ProxyStatusResponse { enforce_proxy, accounts: account_statuses }))
}

async fn persist_proxy_url(
    state: &AppState,
    account_id: &str,
    proxy_url: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    // DB first (no split-brain)
    if let Some(repo) = state.repository() {
        repo.update_proxy_url(account_id, proxy_url)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // JSON file dual-write (best-effort — VPS may not have JSON files)
    let aid = account_id.to_string();
    let purl = proxy_url.map(String::from);
    let json_result = tokio::task::spawn_blocking(move || {
        match account::load_account(&aid) {
            Ok(mut acc) => {
                acc.proxy_url = purl;
                account::save_account(&acc)
            },
            Err(_) => Ok(()), // JSON file doesn't exist — DB-only mode
        }
    })
    .await;
    if let Err(e) = json_result {
        tracing::warn!("JSON dual-write spawn_blocking panicked: {e}");
    } else if let Ok(Err(e)) = json_result {
        tracing::warn!("JSON dual-write failed: {e}");
    }

    Ok(())
}

async fn get_account_email(
    state: &AppState,
    account_id: &str,
) -> Result<String, (StatusCode, String)> {
    if let Some(repo) = state.repository() {
        let account = repo
            .get_account(account_id)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
        Ok(account.email)
    } else {
        let aid = account_id.to_string();
        tokio::task::spawn_blocking(move || account::load_account(&aid).map(|a| a.email))
            .await
            .map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))
            })?
            .map_err(|e| (StatusCode::NOT_FOUND, e))
    }
}
