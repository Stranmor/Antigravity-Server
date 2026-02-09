//! JSON to PostgreSQL migration utilities.
//!
//! This module provides functions to migrate account data from JSON files
//! to PostgreSQL database, with verification and statistics tracking.
#![allow(clippy::arithmetic_side_effects, reason = "counter increments are safe")]

use crate::models::Account;
use crate::modules::account::{get_accounts_dir, load_account_index};
use crate::modules::account_pg::PostgresAccountRepository;
use crate::modules::repository::AccountRepository;
use tracing::{error, info, warn};

/// Key used to track migration completion in app_settings.
const MIGRATION_KEY: &str = "json_migration_completed";

/// Migrates accounts from JSON files to PostgreSQL.
pub async fn migrate_json_to_postgres(
    repo: &PostgresAccountRepository,
) -> Result<MigrationStats, String> {
    let migrated: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key = $1")
            .bind(MIGRATION_KEY)
            .fetch_optional(repo.pool())
            .await
            .map_err(|e| format!("Failed to check migration status: {}", e))?;

    if migrated.is_some() {
        info!("JSON migration already completed, skipping");
        return Ok(MigrationStats::default());
    }

    let mut stats = MigrationStats::default();

    info!("Starting JSON to PostgreSQL migration...");

    let index = load_account_index().map_err(|e| format!("Failed to load account index: {}", e))?;
    let accounts_dir =
        get_accounts_dir().map_err(|e| format!("Failed to get accounts dir: {}", e))?;

    for summary in &index.accounts {
        let account_path = accounts_dir.join(format!("{}.json", summary.id));

        if !account_path.exists() {
            warn!("Account file missing: {}", summary.id);
            stats.skipped += 1;
            continue;
        }

        let content = match tokio::fs::read_to_string(&account_path).await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to read {}: {}", summary.id, e);
                stats.failed += 1;
                continue;
            },
        };

        let account: Account = match serde_json::from_str(&content) {
            Ok(a) => a,
            Err(e) => {
                error!("Failed to parse {}: {}", summary.id, e);
                stats.failed += 1;
                continue;
            },
        };

        match repo.get_account_by_email(&account.email).await {
            Ok(Some(existing)) => {
                let mut updated = existing.clone();
                updated.token = account.token.clone();
                updated.disabled = account.disabled;
                updated.disabled_reason = account.disabled_reason.clone();
                updated.disabled_at = account.disabled_at;
                updated.proxy_disabled = account.proxy_disabled;
                updated.proxy_disabled_reason = account.proxy_disabled_reason.clone();
                updated.proxy_disabled_at = account.proxy_disabled_at;
                updated.protected_models = account.protected_models.clone();

                info!("Account {} already exists, updating...", account.email);
                if let Err(e) = repo.update_account(&updated).await {
                    error!("Failed to update {}: {}", account.email, e);
                    stats.failed += 1;
                } else {
                    stats.updated += 1;
                }
            },
            Ok(None) => {
                match repo
                    .create_account(
                        account.email.clone(),
                        account.name.clone(),
                        account.token.clone(),
                    )
                    .await
                {
                    Ok(created) => {
                        let mut updated = created;
                        updated.disabled = account.disabled;
                        updated.disabled_reason = account.disabled_reason.clone();
                        updated.disabled_at = account.disabled_at;
                        updated.proxy_disabled = account.proxy_disabled;
                        updated.proxy_disabled_reason = account.proxy_disabled_reason.clone();
                        updated.proxy_disabled_at = account.proxy_disabled_at;
                        updated.protected_models = account.protected_models.clone();

                        if let Err(e) = repo.update_account(&updated).await {
                            warn!(
                                "Created but failed to update flags for {}: {}",
                                account.email, e
                            );
                        }

                        if let Some(quota) = &account.quota {
                            if let Err(e) =
                                repo.update_quota(&updated.id, quota.clone(), None).await
                            {
                                warn!("Failed to migrate quota for {}: {}", account.email, e);
                            }
                        }

                        stats.migrated += 1;
                        info!("Migrated: {}", account.email);
                    },
                    Err(e) => {
                        error!("Failed to create {}: {}", account.email, e);
                        stats.failed += 1;
                    },
                }
            },
            Err(e) => {
                error!("Failed to check existence of {}: {}", account.email, e);
                stats.failed += 1;
            },
        }
    }

    if let Some(current_json_id) = &index.current_account_id {
        let accounts_dir =
            get_accounts_dir().map_err(|e| format!("Failed to get accounts dir: {}", e))?;
        let current_account_path = accounts_dir.join(format!("{}.json", current_json_id));

        if current_account_path.exists() {
            match tokio::fs::read_to_string(&current_account_path).await {
                Ok(content) => match serde_json::from_str::<Account>(&content) {
                    Ok(account) => match repo.get_account_by_email(&account.email).await {
                        Ok(Some(pg_account)) => {
                            if let Err(e) = repo.set_current_account_id(&pg_account.id).await {
                                warn!("Failed to set current account ID: {}", e);
                            }
                        },
                        Ok(None) => {
                            warn!(
                                "Current account {} not found in PostgreSQL after migration",
                                account.email
                            );
                        },
                        Err(e) => {
                            warn!("Failed to lookup current account {}: {}", account.email, e);
                        },
                    },
                    Err(e) => {
                        warn!("Failed to parse current account JSON {}: {}", current_json_id, e);
                    },
                },
                Err(e) => {
                    warn!("Failed to read current account file {}: {}", current_json_id, e);
                },
            }
        }
    }

    info!(
        "Migration complete: {} migrated, {} updated, {} skipped, {} failed",
        stats.migrated, stats.updated, stats.skipped, stats.failed
    );

    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = $2",
    )
    .bind(MIGRATION_KEY)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(repo.pool())
    .await
    .map_err(|e| format!("Failed to mark migration complete: {}", e))?;

    Ok(stats)
}

/// Statistics from a migration run.
#[derive(Default, Debug)]
pub struct MigrationStats {
    /// Number of accounts successfully migrated.
    pub migrated: usize,
    /// Number of existing accounts updated.
    pub updated: usize,
    /// Number of accounts skipped.
    pub skipped: usize,
    /// Number of accounts that failed to migrate.
    pub failed: usize,
}

/// Verifies migration completeness by comparing JSON and PostgreSQL.
pub async fn verify_migration(
    repo: &PostgresAccountRepository,
) -> Result<VerificationResult, String> {
    let json_index = load_account_index().map_err(|e| e.to_string())?;
    let pg_accounts = repo.list_accounts().await.map_err(|e| e.to_string())?;

    let json_count = json_index.accounts.len();
    let pg_count = pg_accounts.len();

    let json_emails: std::collections::HashSet<_> =
        json_index.accounts.iter().map(|a| a.email.clone()).collect();
    let pg_emails: std::collections::HashSet<_> =
        pg_accounts.iter().map(|a| a.email.clone()).collect();

    let missing_in_pg: Vec<_> = json_emails.difference(&pg_emails).cloned().collect();
    let extra_in_pg: Vec<_> = pg_emails.difference(&json_emails).cloned().collect();
    let is_complete = json_count == pg_count && missing_in_pg.is_empty();

    Ok(VerificationResult { json_count, pg_count, missing_in_pg, extra_in_pg, is_complete })
}

/// Result of migration verification.
#[derive(Debug)]
pub struct VerificationResult {
    /// Number of accounts in JSON files.
    pub json_count: usize,
    /// Number of accounts in PostgreSQL.
    pub pg_count: usize,
    /// Accounts in JSON but not in PostgreSQL.
    pub missing_in_pg: Vec<String>,
    /// Accounts in PostgreSQL but not in JSON.
    pub extra_in_pg: Vec<String>,
    /// Whether migration is complete.
    pub is_complete: bool,
}
