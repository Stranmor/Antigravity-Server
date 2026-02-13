//! Targeted (single-column) update operations for PostgreSQL accounts.
//!
//! These avoid full-row read-modify-write, eliminating race conditions
//! when concurrent operations update different fields on the same account.

use crate::modules::account_pg_events::log_event_internal_impl;
use crate::modules::account_pg_helpers::map_sqlx_err;
use crate::modules::repository::{AccountEventType, RepoResult, RepositoryError};
use sqlx::postgres::PgPool;
use uuid::Uuid;

pub(crate) async fn update_token_credentials_impl(
    pool: &PgPool,
    account_id: &str,
    access_token: &str,
    refresh_token: Option<&str>,
    expiry: chrono::DateTime<chrono::Utc>,
) -> RepoResult<()> {
    let uuid = Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
    let expiry_timestamp = expiry.timestamp();

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    let result = if let Some(rt) = refresh_token {
        sqlx::query(
            "UPDATE tokens SET access_token = $2, expiry_timestamp = $3, refresh_token = $4 WHERE account_id = $1",
        )
        .bind(uuid)
        .bind(access_token)
        .bind(expiry_timestamp)
        .bind(rt)
        .execute(&mut *tx)
        .await
    } else {
        sqlx::query(
            "UPDATE tokens SET access_token = $2, expiry_timestamp = $3 WHERE account_id = $1",
        )
        .bind(uuid)
        .bind(access_token)
        .bind(expiry_timestamp)
        .execute(&mut *tx)
        .await
    }
    .map_err(map_sqlx_err)?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound(account_id.to_string()));
    }

    let expires_in = expiry.signed_duration_since(chrono::Utc::now()).num_seconds();

    log_event_internal_impl(
        &mut tx,
        account_id.to_string(),
        AccountEventType::TokenRefreshed,
        serde_json::json!({"expires_in": expires_in}),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx_err)
}

pub(crate) async fn update_project_id_impl(
    pool: &PgPool,
    account_id: &str,
    project_id: &str,
) -> RepoResult<()> {
    let uuid = Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    let result = sqlx::query("UPDATE tokens SET project_id = $2 WHERE account_id = $1")
        .bind(uuid)
        .bind(project_id)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound(account_id.to_string()));
    }

    log_event_internal_impl(
        &mut tx,
        account_id.to_string(),
        AccountEventType::Updated,
        serde_json::json!({"project_id": project_id}),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx_err)
}

pub(crate) async fn update_name_impl(
    pool: &PgPool,
    account_id: &str,
    name: &str,
) -> RepoResult<()> {
    let uuid = Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    let result = sqlx::query("UPDATE accounts SET name = $2 WHERE id = $1")
        .bind(uuid)
        .bind(name)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound(account_id.to_string()));
    }

    log_event_internal_impl(
        &mut tx,
        account_id.to_string(),
        AccountEventType::Updated,
        serde_json::json!({"name": name}),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx_err)
}

pub(crate) async fn update_proxy_url_impl(
    pool: &PgPool,
    account_id: &str,
    proxy_url: Option<&str>,
) -> RepoResult<()> {
    let uuid = Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    let result = sqlx::query("UPDATE accounts SET proxy_url = $2 WHERE id = $1")
        .bind(uuid)
        .bind(proxy_url)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound(account_id.to_string()));
    }

    log_event_internal_impl(
        &mut tx,
        account_id.to_string(),
        AccountEventType::Updated,
        serde_json::json!({"proxy_url": proxy_url}),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx_err)
}

pub(crate) async fn set_account_disabled_impl(
    pool: &PgPool,
    account_id: &str,
    reason: &str,
    disabled_at: chrono::DateTime<chrono::Utc>,
) -> RepoResult<()> {
    let uuid = Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    let result = sqlx::query(
        "UPDATE accounts SET disabled = true, disabled_reason = $2, disabled_at = $3 WHERE id = $1",
    )
    .bind(uuid)
    .bind(reason)
    .bind(disabled_at)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx_err)?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound(account_id.to_string()));
    }

    log_event_internal_impl(
        &mut tx,
        account_id.to_string(),
        AccountEventType::Disabled,
        serde_json::json!({"reason": reason}),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx_err)
}
