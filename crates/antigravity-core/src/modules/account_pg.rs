use crate::models::{Account, QuotaData, TokenData};
use crate::modules::repository::{
    AccountEvent, AccountEventType, AccountHealth, AccountRepository, RepoResult, RepositoryError,
    RequestLog,
};
use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use std::collections::HashSet;
use std::time::Duration;
use uuid::Uuid;

pub struct PostgresAccountRepository {
    pool: PgPool,
}

impl PostgresAccountRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .min_connections(2)
            .acquire_timeout(Duration::from_secs(30))
            .idle_timeout(Duration::from_secs(300))
            .connect(database_url)
            .await?;
        Ok(Self::new(pool))
    }

    pub async fn run_migrations(&self) -> RepoResult<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| RepositoryError::Database(e.to_string()))
    }
}

fn map_sqlx_err(e: sqlx::Error) -> RepositoryError {
    RepositoryError::Database(e.to_string())
}

#[async_trait]
impl AccountRepository for PostgresAccountRepository {
    async fn list_accounts(&self) -> RepoResult<Vec<Account>> {
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
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_err)?;

        let mut accounts = Vec::with_capacity(rows.len());
        for row in rows {
            accounts.push(row_to_account(&row)?);
        }
        Ok(accounts)
    }

    async fn get_account(&self, id: &str) -> RepoResult<Account> {
        let uuid = Uuid::parse_str(id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
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
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_err)?
        .ok_or_else(|| RepositoryError::NotFound(id.to_string()))?;

        row_to_account(&row)
    }

    async fn get_account_by_email(&self, email: &str) -> RepoResult<Option<Account>> {
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
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_err)?;

        match row {
            Some(r) => Ok(Some(row_to_account(&r)?)),
            None => Ok(None),
        }
    }

    async fn create_account(
        &self,
        email: String,
        name: Option<String>,
        token: TokenData,
    ) -> RepoResult<Account> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let protected_models: Vec<String> = vec![];

        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;

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

        self.log_event_internal(
            &mut tx,
            id.to_string(),
            AccountEventType::Created,
            serde_json::json!({"email": email}),
        )
        .await?;

        tx.commit().await.map_err(map_sqlx_err)?;

        self.get_account(&id.to_string()).await
    }

    async fn update_account(&self, account: &Account) -> RepoResult<()> {
        let id =
            Uuid::parse_str(&account.id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
        let protected: Vec<String> = account.protected_models.iter().cloned().collect();

        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;

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

        self.log_event_internal(
            &mut tx,
            id.to_string(),
            AccountEventType::Updated,
            serde_json::json!({"email": account.email}),
        )
        .await?;

        tx.commit().await.map_err(map_sqlx_err)
    }

    async fn upsert_account(
        &self,
        email: String,
        name: Option<String>,
        token: TokenData,
    ) -> RepoResult<Account> {
        let id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let protected_models: Vec<String> = vec![];

        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;

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

        self.log_event_internal(
            &mut tx,
            account_id.to_string(),
            AccountEventType::Updated,
            serde_json::json!({"email": email, "operation": "upsert"}),
        )
        .await?;

        tx.commit().await.map_err(map_sqlx_err)?;

        self.get_account_by_email(&email)
            .await?
            .ok_or(RepositoryError::NotFound(email))
    }

    async fn delete_account(&self, id: &str) -> RepoResult<()> {
        let uuid = Uuid::parse_str(id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;

        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;

        // Log deletion event BEFORE deleting (to preserve account_id reference)
        self.log_event_internal(
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

    async fn delete_accounts(&self, ids: &[String]) -> RepoResult<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let uuids: Vec<Uuid> = ids
            .iter()
            .map(|id| Uuid::parse_str(id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| RepositoryError::NotFound(e.to_string()))?;

        let mut tx = self.pool.begin().await.map_err(map_sqlx_err)?;

        for (uuid, id) in uuids.iter().zip(ids.iter()) {
            self.log_event_internal(
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

    async fn update_quota(&self, account_id: &str, quota: QuotaData) -> RepoResult<()> {
        let uuid =
            Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
        sqlx::query(
            r#"INSERT INTO quotas (account_id, is_forbidden, models, fetched_at)
               VALUES ($1, $2, $3, NOW())
               ON CONFLICT (account_id) DO UPDATE SET is_forbidden = $2, models = $3, fetched_at = NOW()"#,
        )
        .bind(uuid)
        .bind(quota.is_forbidden)
        .bind(serde_json::to_value(&quota.models).unwrap())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;

        self.log_event(AccountEvent {
            account_id: account_id.to_string(),
            event_type: AccountEventType::QuotaUpdated,
            metadata: serde_json::json!({"is_forbidden": quota.is_forbidden}),
            created_at: chrono::Utc::now(),
        })
        .await?;

        Ok(())
    }

    async fn get_current_account_id(&self) -> RepoResult<Option<String>> {
        let row = sqlx::query("SELECT value FROM app_settings WHERE key = 'current_account_id'")
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_err)?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    async fn set_current_account_id(&self, id: &str) -> RepoResult<()> {
        sqlx::query(
            r#"INSERT INTO app_settings (key, value) VALUES ('current_account_id', $1)
               ON CONFLICT (key) DO UPDATE SET value = $1"#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn log_event(&self, event: AccountEvent) -> RepoResult<()> {
        let uuid = Uuid::parse_str(&event.account_id)
            .map_err(|e| RepositoryError::NotFound(e.to_string()))?;
        sqlx::query(
            "INSERT INTO account_events (account_id, event_type, metadata) VALUES ($1, $2, $3)",
        )
        .bind(uuid)
        .bind(event.event_type.as_str())
        .bind(event.metadata)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn log_request(&self, request: RequestLog) -> RepoResult<()> {
        let uuid = Uuid::parse_str(&request.account_id)
            .map_err(|e| RepositoryError::NotFound(e.to_string()))?;
        sqlx::query(
            r#"INSERT INTO requests (account_id, model, tokens_in, tokens_out, latency_ms, status_code, error_type)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(uuid)
        .bind(&request.model)
        .bind(request.tokens_in)
        .bind(request.tokens_out)
        .bind(request.latency_ms)
        .bind(request.status_code)
        .bind(&request.error_type)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_err)?;
        Ok(())
    }

    async fn get_account_health(&self, account_id: &str) -> RepoResult<AccountHealth> {
        let uuid =
            Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
        let row = sqlx::query("SELECT * FROM account_health WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
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

    async fn get_events(&self, account_id: &str, limit: i64) -> RepoResult<Vec<AccountEvent>> {
        let uuid =
            Uuid::parse_str(account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
        let rows = sqlx::query(
            "SELECT event_type, metadata, created_at FROM account_events WHERE account_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(uuid)
        .bind(limit)
        .fetch_all(&self.pool)
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
}

impl PostgresAccountRepository {
    async fn log_event_internal(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        account_id: String,
        event_type: AccountEventType,
        metadata: serde_json::Value,
    ) -> RepoResult<()> {
        let uuid =
            Uuid::parse_str(&account_id).map_err(|e| RepositoryError::NotFound(e.to_string()))?;
        sqlx::query(
            "INSERT INTO account_events (account_id, event_type, metadata) VALUES ($1, $2, $3)",
        )
        .bind(uuid)
        .bind(event_type.as_str())
        .bind(metadata)
        .execute(&mut **tx)
        .await
        .map_err(map_sqlx_err)?;
        Ok(())
    }
}

fn row_to_account(row: &sqlx::postgres::PgRow) -> RepoResult<Account> {
    let id: Uuid = row.get("id");
    let protected_json: serde_json::Value = row.get("protected_models");
    let protected: HashSet<String> = serde_json::from_value(protected_json)
        .map_err(|e| RepositoryError::Serialization(e.to_string()))?;

    let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
    let last_used: chrono::DateTime<chrono::Utc> = row.get("last_used_at");
    let disabled_at: Option<chrono::DateTime<chrono::Utc>> = row.get("disabled_at");
    let proxy_disabled_at: Option<chrono::DateTime<chrono::Utc>> = row.get("proxy_disabled_at");
    let expiry_timestamp: i64 = row.get("expiry_timestamp");

    // Calculate expires_in from stored absolute timestamp
    let now = chrono::Utc::now().timestamp();
    let expires_in = (expiry_timestamp - now).max(0);

    Ok(Account {
        id: id.to_string(),
        email: row.get("email"),
        name: row.get("name"),
        token: TokenData {
            access_token: row.get("access_token"),
            refresh_token: row.get("refresh_token"),
            expires_in,
            expiry_timestamp,
            token_type: "Bearer".to_string(),
            email: row.get("token_email"),
            project_id: row.get("project_id"),
            session_id: None,
        },
        quota: None,
        disabled: row.get("disabled"),
        disabled_reason: row.get("disabled_reason"),
        disabled_at: disabled_at.map(|dt| dt.timestamp()),
        proxy_disabled: row.get("proxy_disabled"),
        proxy_disabled_reason: row.get("proxy_disabled_reason"),
        proxy_disabled_at: proxy_disabled_at.map(|dt| dt.timestamp()),
        protected_models: protected,
        created_at: created_at.timestamp(),
        last_used: last_used.timestamp(),
    })
}

fn parse_event_type(s: &str) -> AccountEventType {
    match s {
        "account_created" => AccountEventType::Created,
        "account_updated" => AccountEventType::Updated,
        "account_deleted" => AccountEventType::Deleted,
        "account_disabled" => AccountEventType::Disabled,
        "account_enabled" => AccountEventType::Enabled,
        "token_refreshed" => AccountEventType::TokenRefreshed,
        "quota_updated" => AccountEventType::QuotaUpdated,
        "rate_limited" => AccountEventType::RateLimited,
        "model_protected" => AccountEventType::ModelProtected,
        "model_unprotected" => AccountEventType::ModelUnprotected,
        "phone_verification_required" => AccountEventType::PhoneVerificationRequired,
        _ => AccountEventType::Updated,
    }
}
