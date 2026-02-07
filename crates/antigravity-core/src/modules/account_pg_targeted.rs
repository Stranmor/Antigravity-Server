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
    expires_in: i64,
    expiry_timestamp: i64,
) -> RepoResult<()> {
    let uuid = Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    let result = sqlx::query(
        "UPDATE tokens SET access_token = $2, expiry_timestamp = $3 WHERE account_id = $1",
    )
    .bind(uuid)
    .bind(access_token)
    .bind(expiry_timestamp)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx_err)?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound(account_id.to_string()));
    }

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

pub(crate) async fn set_account_disabled_impl(
    pool: &PgPool,
    account_id: &str,
    reason: &str,
    disabled_at: i64,
) -> RepoResult<()> {
    let uuid = Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
    let ts = chrono::DateTime::from_timestamp(disabled_at, 0)
        .ok_or_else(|| RepositoryError::Database(format!("Invalid timestamp: {}", disabled_at)))?;

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    let result = sqlx::query(
        "UPDATE accounts SET disabled = true, disabled_reason = $2, disabled_at = $3 WHERE id = $1",
    )
    .bind(uuid)
    .bind(reason)
    .bind(ts)
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
