//! Account query operations for PostgreSQL.

use crate::models::Account;
use crate::modules::account_pg_helpers::{map_sqlx_err, row_to_account};
use crate::modules::repository::{RepoResult, RepositoryError};
use sqlx::postgres::PgPool;
use uuid::Uuid;

/// Get an account by ID.
pub(crate) async fn get_account_impl(pool: &PgPool, id: &str) -> RepoResult<Account> {
    let uuid = Uuid::parse_str(id).map_err(|err| RepositoryError::NotFound(err.to_string()))?;
    let row = sqlx::query(
        r#"
        SELECT a.id, a.email, a.name, a.disabled, a.disabled_reason, a.disabled_at,
               a.proxy_disabled, a.proxy_disabled_reason, a.proxy_disabled_at,
               a.protected_models, a.created_at, a.last_used_at,
               t.access_token, t.refresh_token, t.expiry_timestamp, t.project_id, t.email as token_email
        FROM accounts a
        LEFT JOIN tokens t ON a.id = t.account_id
        WHERE a.id = $1
        "#,
    )
    .bind(uuid)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx_err)?
    .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;

    row_to_account(&row)
}

/// Get an account by email address.
pub(crate) async fn get_account_by_email_impl(
    pool: &PgPool,
    email: &str,
) -> RepoResult<Option<Account>> {
    let row = sqlx::query(
        r#"
        SELECT a.id, a.email, a.name, a.disabled, a.disabled_reason, a.disabled_at,
               a.proxy_disabled, a.proxy_disabled_reason, a.proxy_disabled_at,
               a.protected_models, a.created_at, a.last_used_at,
               t.access_token, t.refresh_token, t.expiry_timestamp, t.project_id, t.email as token_email
        FROM accounts a
        LEFT JOIN tokens t ON a.id = t.account_id
        WHERE a.email = $1
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(map_sqlx_err)?;

    match row {
        Some(row_data) => Ok(Some(row_to_account(&row_data)?)),
        None => Ok(None),
    }
}

/// List all accounts.
pub(crate) async fn list_accounts_impl(pool: &PgPool) -> RepoResult<Vec<Account>> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.email, a.name, a.disabled, a.disabled_reason, a.disabled_at,
               a.proxy_disabled, a.proxy_disabled_reason, a.proxy_disabled_at,
               a.protected_models, a.created_at, a.last_used_at,
               t.access_token, t.refresh_token, t.expiry_timestamp, t.project_id, t.email as token_email
        FROM accounts a
        LEFT JOIN tokens t ON a.id = t.account_id
        ORDER BY a.created_at
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_err)?;

    let mut accounts = Vec::with_capacity(rows.len());
    for row in rows {
        accounts.push(row_to_account(&row)?);
    }
    Ok(accounts)
}
