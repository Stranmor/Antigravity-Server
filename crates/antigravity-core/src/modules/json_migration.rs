use crate::models::Account;
use crate::modules::account::{get_accounts_dir, load_account_index};
use crate::modules::account_pg::PostgresAccountRepository;
use crate::modules::repository::AccountRepository;
use tracing::{error, info, warn};

pub async fn migrate_json_to_postgres(
    repo: &PostgresAccountRepository,
) -> Result<MigrationStats, String> {
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
            }
        };

        let account: Account = match serde_json::from_str(&content) {
            Ok(a) => a,
            Err(e) => {
                error!("Failed to parse {}: {}", summary.id, e);
                stats.failed += 1;
                continue;
            }
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
            }
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
                            let _ = repo.update_quota(&updated.id, quota.clone()).await;
                        }

                        stats.migrated += 1;
                        info!("Migrated: {}", account.email);
                    }
                    Err(e) => {
                        error!("Failed to create {}: {}", account.email, e);
                        stats.failed += 1;
                    }
                }
            }
            Err(e) => {
                error!("Failed to check existence of {}: {}", account.email, e);
                stats.failed += 1;
            }
        }
    }

    if let Some(current_json_id) = &index.current_account_id {
        let accounts_dir =
            get_accounts_dir().map_err(|e| format!("Failed to get accounts dir: {}", e))?;
        let current_account_path = accounts_dir.join(format!("{}.json", current_json_id));

        if current_account_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&current_account_path).await {
                if let Ok(account) = serde_json::from_str::<Account>(&content) {
                    if let Ok(Some(pg_account)) = repo.get_account_by_email(&account.email).await {
                        if let Err(e) = repo.set_current_account_id(&pg_account.id).await {
                            warn!("Failed to set current account ID: {}", e);
                        }
                    }
                }
            }
        }
    }

    info!(
        "Migration complete: {} migrated, {} updated, {} skipped, {} failed",
        stats.migrated, stats.updated, stats.skipped, stats.failed
    );

    Ok(stats)
}

#[derive(Default, Debug)]
pub struct MigrationStats {
    pub migrated: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub async fn verify_migration(
    repo: &PostgresAccountRepository,
) -> Result<VerificationResult, String> {
    let json_index = load_account_index().map_err(|e| e.to_string())?;
    let pg_accounts = repo.list_accounts().await.map_err(|e| e.to_string())?;

    let json_count = json_index.accounts.len();
    let pg_count = pg_accounts.len();

    let json_emails: std::collections::HashSet<_> = json_index
        .accounts
        .iter()
        .map(|a| a.email.clone())
        .collect();
    let pg_emails: std::collections::HashSet<_> =
        pg_accounts.iter().map(|a| a.email.clone()).collect();

    let missing_in_pg: Vec<_> = json_emails.difference(&pg_emails).cloned().collect();
    let extra_in_pg: Vec<_> = pg_emails.difference(&json_emails).cloned().collect();
    let is_complete = json_count == pg_count && missing_in_pg.is_empty();

    Ok(VerificationResult {
        json_count,
        pg_count,
        missing_in_pg,
        extra_in_pg,
        is_complete,
    })
}

#[derive(Debug)]
pub struct VerificationResult {
    pub json_count: usize,
    pub pg_count: usize,
    pub missing_in_pg: Vec<String>,
    pub extra_in_pg: Vec<String>,
    pub is_complete: bool,
}
