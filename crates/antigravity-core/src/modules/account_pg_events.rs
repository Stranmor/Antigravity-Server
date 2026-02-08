//! Account event logging and quota management for PostgreSQL.

use crate::models::QuotaData;
use crate::modules::account_pg_helpers::{map_sqlx_err, parse_event_type};
use crate::modules::repository::{
    AccountEvent, AccountEventType, AccountHealth, RepoResult, RepositoryError, RequestLog,
};
use sqlx::postgres::PgPool;
use sqlx::Row;
use uuid::Uuid;

/// Update quota data for an account.
pub(crate) async fn update_quota_impl(
    pool: &PgPool,
    account_id: &str,
    quota: QuotaData,
) -> RepoResult<()> {
    let uuid =
        Uuid::parse_str(account_id).map_err(|err| RepositoryError::NotFound(err.to_string()))?;
    let models_json = serde_json::to_value(&quota.models)
        .map_err(|err| RepositoryError::Serialization(err.to_string()))?;

    sqlx::query(
        r#"INSERT INTO quotas (account_id, is_forbidden, models, fetched_at)
           VALUES ($1, $2, $3, NOW())
           ON CONFLICT (account_id) DO UPDATE SET is_forbidden = $2, models = $3, fetched_at = NOW()"#,
    )
    .bind(uuid)
    .bind(quota.is_forbidden)
    .bind(models_json)
    .execute(pool)
    .await
    .map_err(map_sqlx_err)?;

    // Persist subscription tier to tokens table (source of truth for account tier display)
    if let Some(ref tier) = quota.subscription_tier {
        sqlx::query("UPDATE tokens SET tier = $1 WHERE account_id = $2")
            .bind(tier)
            .bind(uuid)
            .execute(pool)
            .await
            .map_err(map_sqlx_err)?;
    }

    log_event_impl(
        pool,
        AccountEvent {
            account_id: account_id.to_string(),
            event_type: AccountEventType::QuotaUpdated,
            metadata: serde_json::json!({"is_forbidden": quota.is_forbidden}),
            created_at: chrono::Utc::now(),
        },
    )
    .await?;

    Ok(())
}

/// Get the currently selected account ID.
pub(crate) async fn get_current_account_id_impl(pool: &PgPool) -> RepoResult<Option<String>> {
    let row = sqlx::query("SELECT value FROM app_settings WHERE key = 'current_account_id'")
        .fetch_optional(pool)
        .await
        .map_err(map_sqlx_err)?;
    Ok(row.map(|row_data| row_data.get::<String, _>("value")))
}

/// Set the currently selected account ID.
pub(crate) async fn set_current_account_id_impl(pool: &PgPool, id: &str) -> RepoResult<()> {
    sqlx::query(
        r#"INSERT INTO app_settings (key, value) VALUES ('current_account_id', $1)
           ON CONFLICT (key) DO UPDATE SET value = $1"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}

/// Log an account event.
pub(crate) async fn log_event_impl(pool: &PgPool, event: AccountEvent) -> RepoResult<()> {
    let uuid = Uuid::parse_str(&event.account_id)
        .map_err(|err| RepositoryError::NotFound(err.to_string()))?;
    sqlx::query(
        "INSERT INTO account_events (account_id, event_type, metadata) VALUES ($1, $2, $3)",
    )
    .bind(uuid)
    .bind(event.event_type.as_str())
    .bind(event.metadata)
    .execute(pool)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}

/// Log a request for analytics.
pub(crate) async fn log_request_impl(pool: &PgPool, request: RequestLog) -> RepoResult<()> {
    let uuid = Uuid::parse_str(&request.account_id)
        .map_err(|err| RepositoryError::NotFound(err.to_string()))?;
    sqlx::query(
        r#"INSERT INTO requests (account_id, model, tokens_in, tokens_out, cached_tokens, latency_ms, status_code, error_type)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(uuid)
    .bind(&request.model)
    .bind(request.tokens_in)
    .bind(request.tokens_out)
    .bind(request.cached_tokens)
    .bind(request.latency_ms)
    .bind(request.status_code)
    .bind(&request.error_type)
    .execute(pool)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}

/// Get health metrics for an account.
pub(crate) async fn get_account_health_impl(
    pool: &PgPool,
    account_id: &str,
) -> RepoResult<AccountHealth> {
    let uuid =
        Uuid::parse_str(account_id).map_err(|err| RepositoryError::NotFound(err.to_string()))?;
    let row = sqlx::query("SELECT * FROM account_health WHERE id = $1")
        .bind(uuid)
        .fetch_optional(pool)
        .await
        .map_err(map_sqlx_err)?
        .ok_or_else(|| RepositoryError::NotFound(account_id.to_string()))?;

    Ok(AccountHealth {
        account_id: account_id.to_string(),
        email: row.get("email"),
        total_requests: row.get::<i64, _>("total_requests"),
        successful_requests: row.get::<i64, _>("successful_requests"),
        rate_limited_requests: row.get::<i64, _>("rate_limited_requests"),
        success_rate_pct: row.get::<Option<f64>, _>("success_rate_pct").unwrap_or(0.0),
        avg_latency_ms: row.get::<Option<f64>, _>("avg_latency_ms"),
    })
}

/// Get recent events for an account.
pub(crate) async fn get_events_impl(
    pool: &PgPool,
    account_id: &str,
    limit: i64,
) -> RepoResult<Vec<AccountEvent>> {
    let uuid =
        Uuid::parse_str(account_id).map_err(|err| RepositoryError::NotFound(err.to_string()))?;
    let rows = sqlx::query(
        "SELECT event_type, metadata, created_at FROM account_events WHERE account_id = $1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind(uuid)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(map_sqlx_err)?;

    Ok(rows
        .into_iter()
        .map(|row| AccountEvent {
            account_id: account_id.to_string(),
            event_type: parse_event_type(row.get("event_type")),
            metadata: row.get("metadata"),
            created_at: row.get("created_at"),
        })
        .collect())
}

/// Log an event within a transaction.
pub(crate) async fn log_event_internal_impl(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    account_id: String,
    event_type: AccountEventType,
    metadata: serde_json::Value,
) -> RepoResult<()> {
    let uuid =
        Uuid::parse_str(&account_id).map_err(|err| RepositoryError::NotFound(err.to_string()))?;
    sqlx::query(
        "INSERT INTO account_events (account_id, event_type, metadata) VALUES ($1, $2, $3)",
    )
    .bind(uuid)
    .bind(event_type.as_str())
    .bind(metadata)
    .execute(&mut **transaction)
    .await
    .map_err(map_sqlx_err)?;
    Ok(())
}
