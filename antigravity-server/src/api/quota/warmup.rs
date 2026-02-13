//! Warmup and toggle handlers

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use antigravity_core::modules::account;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct ToggleProxyRequest {
    pub account_id: String,
    pub enable: bool,
    pub reason: Option<String>,
}

#[derive(Serialize)]
pub struct ToggleProxyResponse {
    pub success: bool,
    pub account_id: String,
    pub proxy_disabled: bool,
}

pub async fn toggle_proxy_status(
    State(state): State<AppState>,
    Json(payload): Json<ToggleProxyRequest>,
) -> Result<Json<ToggleProxyResponse>, (StatusCode, String)> {
    tracing::info!(
        account_id = %payload.account_id,
        enable = %payload.enable,
        reason = ?payload.reason,
        "Toggling proxy status"
    );

    let proxy_disabled = !payload.enable;

    if let Some(repo) = state.repository() {
        // PostgreSQL path: load by ID directly from DB
        let mut pg_account = repo
            .get_account(&payload.account_id)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

        pg_account.proxy_disabled = proxy_disabled;
        repo.update_account(&pg_account)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Also update file if it exists (best-effort sync)
        let file_account_id = pg_account.id.clone();
        let file_proxy_disabled = proxy_disabled;
        tokio::task::spawn_blocking(move || {
            if let Ok(mut acc) = account::load_account(&file_account_id) {
                acc.proxy_disabled = file_proxy_disabled;
                let _ = account::save_account(&acc);
            }
        })
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))
        })?;
    } else {
        // File-only fallback (no PostgreSQL)
        let account_id = payload.account_id.clone();
        let mut acc = tokio::task::spawn_blocking(move || account::load_account(&account_id))
            .await
            .map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))
            })?
            .map_err(|e| (StatusCode::NOT_FOUND, e))?;

        acc.proxy_disabled = proxy_disabled;

        tokio::task::spawn_blocking(move || account::save_account(&acc))
            .await
            .map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))
            })?
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }

    if let Err(e) = state.reload_accounts().await {
        tracing::warn!("Failed to reload accounts after proxy toggle: {}", e);
    }

    Ok(Json(ToggleProxyResponse { success: true, account_id: payload.account_id, proxy_disabled }))
}

#[derive(Deserialize)]
pub struct WarmupAccountRequest {
    pub account_id: String,
}

#[derive(Serialize)]
pub struct WarmupResponse {
    pub success: bool,
    pub message: String,
}

pub async fn warmup_account(
    State(state): State<AppState>,
    Json(payload): Json<WarmupAccountRequest>,
) -> Result<Json<WarmupResponse>, (StatusCode, String)> {
    let acc = if let Some(repo) = state.repository() {
        repo.get_account(&payload.account_id)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?
    } else {
        let account_id = payload.account_id.clone();
        tokio::task::spawn_blocking(move || account::load_account(&account_id))
            .await
            .map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("spawn_blocking panicked: {e}"))
            })?
            .map_err(|e| (StatusCode::NOT_FOUND, e))?
    };

    let enforce_proxy = state.enforce_proxy().await;
    match account::fetch_quota_with_retry(&acc, state.repository(), enforce_proxy).await {
        Ok(result) => {
            let quota = result.quota;
            let protected_models =
                match account::update_account_quota_async(acc.id.clone(), quota.clone()).await {
                    Ok(updated) => Some(updated.protected_models.iter().cloned().collect()),
                    Err(e) => {
                        tracing::warn!("Failed to update quota for {}: {}", acc.email, e);
                        None
                    },
                };
            if let Some(repo) = state.repository() {
                if let Err(e) = repo.update_quota(&acc.id, quota, protected_models).await {
                    tracing::warn!("DB quota update failed for {}: {}", acc.email, e);
                }
            }
            if let Err(e) = state.reload_accounts().await {
                tracing::warn!("Failed to reload accounts after warmup: {}", e);
            }

            Ok(Json(WarmupResponse {
                success: true,
                message: format!("Account {} warmed up successfully", acc.email),
            }))
        },
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn warmup_all_accounts(
    State(state): State<AppState>,
) -> Result<Json<WarmupResponse>, (StatusCode, String)> {
    let accounts =
        state.list_accounts().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let total = accounts.len();

    struct WarmupResult {
        account_id: String,
        email: String,
        quota: Option<antigravity_types::models::QuotaData>,
    }

    let mut join_set: JoinSet<Result<WarmupResult, String>> = JoinSet::new();
    let semaphore = Arc::new(Semaphore::new(10));
    let repo = state.repository().cloned();
    let enforce_proxy_all = state.enforce_proxy().await;

    for acc in accounts {
        if acc.disabled || acc.proxy_disabled {
            continue;
        }

        let account_id = acc.id.clone();
        let email = acc.email.clone();
        let permit =
            semaphore.clone().acquire_owned().await.map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Semaphore error: {}", e))
            })?;
        let repo_clone = repo.clone();

        join_set.spawn(async move {
            let _permit = permit;
            let result =
                account::fetch_quota_with_retry(&acc, repo_clone.as_ref(), enforce_proxy_all)
                    .await
                    .map_err(|e| format!("{}: {}", email, e))?;

            Ok(WarmupResult { account_id, email, quota: Some(result.quota) })
        });
    }

    let mut success = 0;
    let mut failed = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(warmup_result)) => {
                if let Some(quota) = warmup_result.quota {
                    let protected_models = match account::update_account_quota_async(
                        warmup_result.account_id.clone(),
                        quota.clone(),
                    )
                    .await
                    {
                        Ok(updated) => Some(updated.protected_models.iter().cloned().collect()),
                        Err(e) => {
                            tracing::warn!(
                                "Failed to update quota for {}: {}",
                                warmup_result.account_id,
                                e
                            );
                            None
                        },
                    };
                    if let Some(repo) = state.repository() {
                        if let Err(e) = repo
                            .update_quota(&warmup_result.account_id, quota, protected_models)
                            .await
                        {
                            tracing::warn!(
                                "DB quota update failed for {}: {}",
                                warmup_result.email,
                                e
                            );
                        }
                    }
                }
                success += 1;
            },
            Ok(Err(e)) => {
                tracing::warn!("Warmup failed: {}", e);
                failed += 1;
            },
            Err(e) => {
                tracing::error!("Task panicked: {}", e);
                failed += 1;
            },
        }
    }

    if let Err(e) = state.reload_accounts().await {
        tracing::warn!("Failed to reload accounts after warmup all: {}", e);
    }

    Ok(Json(WarmupResponse {
        success: true,
        message: format!(
            "Warmup complete: {}/{} accounts warmed up ({} failed)",
            success, total, failed
        ),
    }))
}
