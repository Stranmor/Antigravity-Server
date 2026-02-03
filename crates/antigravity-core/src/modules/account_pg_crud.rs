use crate::models::{Account, TokenData};
use crate::modules::account_pg_events::log_event_internal_impl;
use crate::modules::account_pg_helpers::map_sqlx_err;
use crate::modules::account_pg_query::{get_account_by_email_impl, get_account_impl};
use crate::modules::repository::{AccountEventType, RepoResult, RepositoryError};
use sqlx::postgres::PgPool;
use sqlx::Row;
use uuid::Uuid;

pub async fn create_account_impl(
    pool: &PgPool,
    email: String,
    name: Option<String>,
    token: TokenData,
) -> RepoResult<Account> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let protected_models: Vec<String> = vec![];

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    sqlx::query(
        r#"INSERT INTO accounts (id, email, name, protected_models, created_at, last_used_at)
           VALUES ($1, $2, $3, $4, $5, $5)"#,
    )
    .bind(id)
    .bind(&email)
    .bind(&name)
    .bind(serde_json::to_value(&protected_models).unwrap())
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate") {
            RepositoryError::AlreadyExists(email.clone())
        } else {
            map_sqlx_err(e)
        }
    })?;

    sqlx::query(
        r#"INSERT INTO tokens (account_id, access_token, refresh_token, expiry_timestamp, project_id, email)
           VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(id)
    .bind(&token.access_token)
    .bind(&token.refresh_token)
    .bind(token.expiry_timestamp)
    .bind(&token.project_id)
    .bind(&token.email)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx_err)?;

    log_event_internal_impl(
        &mut tx,
        id.to_string(),
        AccountEventType::Created,
        serde_json::json!({"email": email}),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx_err)?;

    get_account_impl(pool, &id.to_string()).await
}

pub async fn update_account_impl(pool: &PgPool, account: &Account) -> RepoResult<()> {
    let id = Uuid::parse_str(&account.id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
    let protected: Vec<String> = account.protected_models.iter().cloned().collect();

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    sqlx::query(
        r#"UPDATE accounts SET email = $2, name = $3, disabled = $4, disabled_reason = $5,
           disabled_at = $6, proxy_disabled = $7, proxy_disabled_reason = $8, proxy_disabled_at = $9,
           protected_models = $10, last_used_at = $11 WHERE id = $1"#,
    )
    .bind(id)
    .bind(&account.email)
    .bind(&account.name)
    .bind(account.disabled)
    .bind(&account.disabled_reason)
    .bind(account.disabled_at.and_then(|ts| chrono::DateTime::from_timestamp(ts, 0)))
    .bind(account.proxy_disabled)
    .bind(&account.proxy_disabled_reason)
    .bind(account.proxy_disabled_at.and_then(|ts| chrono::DateTime::from_timestamp(ts, 0)))
    .bind(serde_json::to_value(&protected).unwrap())
    .bind(chrono::DateTime::from_timestamp(account.last_used, 0))
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx_err)?;

    sqlx::query(
        r#"UPDATE tokens SET access_token = $2, refresh_token = $3, expiry_timestamp = $4,
           project_id = $5, email = $6 WHERE account_id = $1"#,
    )
    .bind(id)
    .bind(&account.token.access_token)
    .bind(&account.token.refresh_token)
    .bind(account.token.expiry_timestamp)
    .bind(&account.token.project_id)
    .bind(&account.token.email)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx_err)?;

    log_event_internal_impl(
        &mut tx,
        id.to_string(),
        AccountEventType::Updated,
        serde_json::json!({"email": account.email}),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx_err)
}

pub async fn upsert_account_impl(
    pool: &PgPool,
    email: String,
    name: Option<String>,
    token: TokenData,
) -> RepoResult<Account> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let protected_models: Vec<String> = vec![];

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    let row = sqlx::query(
        r#"INSERT INTO accounts (id, email, name, protected_models, created_at, last_used_at)
           VALUES ($1, $2, $3, $4, $5, $5)
           ON CONFLICT (email) DO UPDATE SET
               name = EXCLUDED.name,
               last_used_at = NOW()
           RETURNING id"#,
    )
    .bind(id)
    .bind(&email)
    .bind(&name)
    .bind(serde_json::to_value(&protected_models).unwrap())
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_sqlx_err)?;

    let account_id: Uuid = row.get("id");

    sqlx::query(
        r#"INSERT INTO tokens (account_id, access_token, refresh_token, expiry_timestamp, project_id, email)
           VALUES ($1, $2, $3, $4, $5, $6)
           ON CONFLICT (account_id) DO UPDATE SET
               access_token = EXCLUDED.access_token,
               refresh_token = EXCLUDED.refresh_token,
               expiry_timestamp = EXCLUDED.expiry_timestamp,
               project_id = EXCLUDED.project_id,
               email = EXCLUDED.email"#,
    )
    .bind(account_id)
    .bind(&token.access_token)
    .bind(&token.refresh_token)
    .bind(token.expiry_timestamp)
    .bind(&token.project_id)
    .bind(&token.email)
    .execute(&mut *tx)
    .await
    .map_err(map_sqlx_err)?;

    log_event_internal_impl(
        &mut tx,
        account_id.to_string(),
        AccountEventType::Updated,
        serde_json::json!({"email": email, "operation": "upsert"}),
    )
    .await?;

    tx.commit().await.map_err(map_sqlx_err)?;

    get_account_by_email_impl(pool, &email)
        .await?
        .ok_or(RepositoryError::NotFound(email))
}

pub async fn delete_account_impl(pool: &PgPool, id: &str) -> RepoResult<()> {
    let uuid = Uuid::parse_str(id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    log_event_internal_impl(
        &mut tx,
        id.to_string(),
        AccountEventType::Deleted,
        serde_json::json!({}),
    )
    .await?;

    let result = sqlx::query("DELETE FROM accounts WHERE id = $1")
        .bind(uuid)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;

    if result.rows_affected() == 0 {
        return Err(RepositoryError::NotFound(id.to_string()));
    }

    tx.commit().await.map_err(map_sqlx_err)?;
    Ok(())
}

pub async fn delete_accounts_impl(pool: &PgPool, ids: &[String]) -> RepoResult<()> {
    if ids.is_empty() {
        return Ok(());
    }

    let uuids: Vec<Uuid> = ids
        .iter()
        .map(|id| Uuid::parse_str(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| RepositoryError::NotFound(e.to_string()))?;

    let mut tx = pool.begin().await.map_err(map_sqlx_err)?;

    for (uuid, id) in uuids.iter().zip(ids.iter()) {
        log_event_internal_impl(
            &mut tx,
            uuid.to_string(),
            AccountEventType::Deleted,
            serde_json::json!({"batch_delete": true, "original_id": id}),
        )
        .await?;
    }

    sqlx::query("DELETE FROM accounts WHERE id = ANY($1)")
        .bind(&uuids)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_err)?;

    tx.commit().await.map_err(map_sqlx_err)?;

    Ok(())
}
