//! Quota and warmup handlers

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use antigravity_core::modules::account;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct RefreshQuotaRequest {
    pub account_id: String,
}

#[derive(Serialize)]
pub struct QuotaResponse {
    pub account_id: String,
    pub quota: Option<antigravity_types::models::QuotaData>,
}

pub async fn refresh_account_quota(
    State(state): State<AppState>,
    Json(payload): Json<RefreshQuotaRequest>,
) -> Result<Json<QuotaResponse>, (StatusCode, String)> {
    let mut acc = if let Some(repo) = state.repository() {
        repo.get_account(&payload.account_id)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?
    } else {
        account::load_account(&payload.account_id).map_err(|e| (StatusCode::NOT_FOUND, e))?
    };

    match account::fetch_quota_with_retry(&mut acc).await {
        Ok(quota) => {
            if let Err(e) =
                account::update_account_quota_async(payload.account_id.clone(), quota.clone()).await
            {
                tracing::warn!("Failed to update quota protection: {}", e);
                if let Err(e) = account::save_account(&acc) {
                    tracing::warn!("Failed to save account fallback: {}", e);
                }
            }
            if let Some(repo) = state.repository() {
                if let Err(e) = repo.update_quota(&payload.account_id, quota.clone()).await {
                    tracing::warn!("DB quota update failed for {}: {}", payload.account_id, e);
                }
            }
            if let Err(e) = state.reload_accounts().await {
                tracing::warn!("Failed to reload accounts: {}", e);
            }

            Ok(Json(QuotaResponse { account_id: payload.account_id, quota: Some(quota) }))
        },
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn refresh_all_quotas(
    State(state): State<AppState>,
) -> Result<Json<antigravity_types::models::RefreshStats>, (StatusCode, String)> {
    let accounts =
        state.list_accounts().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

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
                if let Err(e) =
                    account::update_account_quota_async(account_id.clone(), quota.clone()).await
                {
                    tracing::warn!("Quota protection update failed for {}: {}", account_id, e);
                }
                if let Some(repo) = state.repository() {
                    if let Err(e) = repo.update_quota(&account_id, quota).await {
                        tracing::warn!("DB quota update failed for {}: {}", account_id, e);
                    }
                }
                success += 1;
            },
            Ok(Err(e)) => {
                tracing::warn!("Quota refresh failed: {}", e);
                failed += 1;
            },
            Err(e) => {
                tracing::error!("Task panicked: {}", e);
                failed += 1;
            },
        }
    }

    if let Err(e) = state.reload_accounts().await {
        tracing::warn!("Failed to reload accounts after quota refresh: {}", e);
    }

    Ok(Json(antigravity_types::models::RefreshStats { total, success, failed }))
}

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

    let mut acc = if let Some(repo) = state.repository() {
        repo.get_account(&payload.account_id)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?
    } else {
        account::load_account(&payload.account_id).map_err(|e| (StatusCode::NOT_FOUND, e))?
    };

    acc.proxy_disabled = !payload.enable;

    if let Some(repo) = state.repository() {
        repo.update_account(&acc)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        account::save_account(&acc).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }

    if let Err(e) = state.reload_accounts().await {
        tracing::warn!("Failed to reload accounts after proxy toggle: {}", e);
    }

    Ok(Json(ToggleProxyResponse {
        success: true,
        account_id: payload.account_id,
        proxy_disabled: acc.proxy_disabled,
    }))
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
    let mut acc = if let Some(repo) = state.repository() {
        repo.get_account(&payload.account_id)
            .await
            .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?
    } else {
        account::load_account(&payload.account_id).map_err(|e| (StatusCode::NOT_FOUND, e))?
    };

    match account::fetch_quota_with_retry(&mut acc).await {
        Ok(_) => {
            if let Some(quota) = acc.quota.clone() {
                if let Err(e) =
                    account::update_account_quota_async(acc.id.clone(), quota.clone()).await
                {
                    tracing::warn!("Failed to update quota for {}: {}", acc.email, e);
                }
                if let Some(repo) = state.repository() {
                    if let Err(e) = repo.update_quota(&acc.id, quota).await {
                        tracing::warn!("DB quota update failed for {}: {}", acc.email, e);
                    }
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

            Ok(WarmupResult { account_id, quota: acc.quota })
        });
    }

    let mut success = 0;
    let mut failed = 0;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(warmup_result)) => {
                if let Some(quota) = warmup_result.quota {
                    if let Err(e) = account::update_account_quota_async(
                        warmup_result.account_id.clone(),
                        quota.clone(),
                    )
                    .await
                    {
                        tracing::warn!(
                            "Failed to update quota for {}: {}",
                            warmup_result.account_id,
                            e
                        );
                    }
                    if let Some(repo) = state.repository() {
                        if let Err(e) = repo.update_quota(&warmup_result.account_id, quota).await {
                            tracing::warn!(
                                "DB quota update failed for {}: {}",
                                warmup_result.account_id,
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
