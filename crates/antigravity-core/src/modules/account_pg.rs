//! PostgreSQL implementation of the account repository.

use crate::models::{Account, QuotaData, TokenData};
use crate::modules::account_pg_crud::{
    create_account_impl, delete_account_impl, delete_accounts_impl, update_account_impl,
    upsert_account_impl,
};
use crate::modules::account_pg_events::{
    get_account_health_impl, get_current_account_id_impl, get_events_impl, log_event_impl,
    log_request_impl, set_current_account_id_impl, update_quota_impl,
};
use crate::modules::account_pg_query::{
    get_account_by_email_impl, get_account_impl, list_accounts_impl,
};
use crate::modules::repository::{
    AccountEvent, AccountHealth, AccountRepository, RepoResult, RepositoryError, RequestLog,
};
use async_trait::async_trait;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

/// PostgreSQL-backed account repository.
pub struct PostgresAccountRepository {
    /// Database connection pool.
    pool: PgPool,
}

impl PostgresAccountRepository {
    /// Create repository with existing pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get reference to the connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Connect to database and create repository.
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

    /// Run database migrations.
    pub async fn run_migrations(&self) -> RepoResult<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|err| RepositoryError::Database(err.to_string()))
    }
}

#[async_trait]
impl AccountRepository for PostgresAccountRepository {
    async fn list_accounts(&self) -> RepoResult<Vec<Account>> {
        list_accounts_impl(&self.pool).await
    }

    async fn get_account(&self, id: &str) -> RepoResult<Account> {
        get_account_impl(&self.pool, id).await
    }

    async fn get_account_by_email(&self, email: &str) -> RepoResult<Option<Account>> {
        get_account_by_email_impl(&self.pool, email).await
    }

    async fn create_account(
        &self,
        email: String,
        name: Option<String>,
        token: TokenData,
    ) -> RepoResult<Account> {
        create_account_impl(&self.pool, email, name, token).await
    }

    async fn update_account(&self, account: &Account) -> RepoResult<()> {
        update_account_impl(&self.pool, account).await
    }

    async fn upsert_account(
        &self,
        email: String,
        name: Option<String>,
        token: TokenData,
    ) -> RepoResult<Account> {
        upsert_account_impl(&self.pool, email, name, token).await
    }

    async fn delete_account(&self, id: &str) -> RepoResult<()> {
        delete_account_impl(&self.pool, id).await
    }

    async fn delete_accounts(&self, ids: &[String]) -> RepoResult<()> {
        delete_accounts_impl(&self.pool, ids).await
    }

    async fn update_quota(&self, account_id: &str, quota: QuotaData) -> RepoResult<()> {
        update_quota_impl(&self.pool, account_id, quota).await
    }

    async fn get_current_account_id(&self) -> RepoResult<Option<String>> {
        get_current_account_id_impl(&self.pool).await
    }

    async fn set_current_account_id(&self, id: &str) -> RepoResult<()> {
        set_current_account_id_impl(&self.pool, id).await
    }

    async fn log_event(&self, event: AccountEvent) -> RepoResult<()> {
        log_event_impl(&self.pool, event).await
    }

    async fn log_request(&self, request: RequestLog) -> RepoResult<()> {
        log_request_impl(&self.pool, request).await
    }

    async fn get_account_health(&self, account_id: &str) -> RepoResult<AccountHealth> {
        get_account_health_impl(&self.pool, account_id).await
    }

    async fn get_events(&self, account_id: &str, limit: i64) -> RepoResult<Vec<AccountEvent>> {
        get_events_impl(&self.pool, account_id, limit).await
    }
}
