use chrono::Utc;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::interval;

use crate::state::AppState;
use antigravity_core::modules::{account, config};
use antigravity_types::models::QuotaData;

/// Persist quota to both JSON and PostgreSQL (if available).
async fn persist_quota(state: &AppState, account_id: &str, email: &str, quota: QuotaData) {
    let protected_models =
        match account::update_account_quota_async(account_id.to_owned(), quota.clone()).await {
            Ok(updated) => Some(updated.protected_models.iter().cloned().collect()),
            Err(e) => {
                tracing::warn!("[QuotaRefresh] Failed to update JSON quota for {account_id}: {e}");
                return;
            },
        };
    if let Some(repo) = state.repository() {
        match repo.get_account_by_email(email).await {
            Ok(Some(pg_account)) => {
                if let Err(e) = repo.update_quota(&pg_account.id, quota, protected_models).await {
                    tracing::warn!("[QuotaRefresh] Failed to update DB quota for {email}: {e}");
                }
            },
            Ok(None) => {
                tracing::warn!("[QuotaRefresh] PG account lookup failed for {email}");
            },
            Err(e) => {
                tracing::warn!("[QuotaRefresh] PG account lookup error for {email}: {e}");
            },
        }
    }
}

/// Start the auto quota refresh scheduler as a background tokio task
pub fn start_quota_refresh(state: AppState) {
    tokio::spawn(async move {
        tracing::info!("[QuotaRefresh] Auto Quota Refresh Scheduler started");

        let mut check_interval = interval(Duration::from_secs(60));
        let mut last_full_refresh: Option<i64> = None;

        loop {
            check_interval.tick().await;

            let app_config = match tokio::task::spawn_blocking(config::load_config).await {
                Ok(res) => match res {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        tracing::warn!("[QuotaRefresh] Failed to load config: {}", e);
                        continue;
                    },
                },
                Err(e) => {
                    tracing::error!("[QuotaRefresh] spawn_blocking panic for load_config: {}", e);
                    continue;
                },
            };

            if !app_config.auto_refresh {
                continue;
            }

            let now = Utc::now().timestamp();
            let interval_minutes =
                if app_config.refresh_interval < 5 { 15 } else { app_config.refresh_interval };
            let interval_secs = i64::from(interval_minutes) * 60_i64;

            let accounts = match tokio::task::spawn_blocking(account::list_accounts).await {
                Ok(res) => match res {
                    Ok(accs) => accs,
                    Err(e) => {
                        tracing::warn!("[QuotaRefresh] Failed to list accounts: {}", e);
                        continue;
                    },
                },
                Err(e) => {
                    tracing::error!("[QuotaRefresh] spawn_blocking panic for list_accounts: {}", e);
                    continue;
                },
            };
            let enabled_accounts: Vec<_> =
                accounts.into_iter().filter(|a| !a.disabled && !a.proxy_disabled).collect();

            let needs_immediate: Vec<_> = enabled_accounts
                .iter()
                .filter(|a| {
                    a.quota.is_none() || a.quota.as_ref().is_some_and(|q| q.needs_refresh())
                })
                .collect();

            let mut already_refreshed: HashSet<String> = HashSet::new();

            if !needs_immediate.is_empty() {
                tracing::info!(
                    "[QuotaRefresh] {} account(s) have expired reset_time, refreshing immediately",
                    needs_immediate.len()
                );
                for acc in &needs_immediate {
                    let mut acc_clone = (*acc).clone();
                    match account::fetch_quota_with_retry(&mut acc_clone).await {
                        Ok(_) => {
                            if let Some(quota) = acc_clone.quota.clone() {
                                persist_quota(&state, &acc_clone.id, &acc_clone.email, quota).await;
                            }
                            already_refreshed.insert(acc_clone.id.clone());
                            tracing::debug!(
                                "[QuotaRefresh] Immediate refresh: {}",
                                acc_clone.email
                            );
                        },
                        Err(e) => {
                            tracing::warn!(
                                "[QuotaRefresh] Failed immediate refresh {}: {}",
                                acc_clone.email,
                                e
                            );
                        },
                    }
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
                if let Err(e) = state.reload_accounts().await {
                    tracing::warn!(
                        "[QuotaRefresh] Failed to reload accounts after immediate refresh: {}",
                        e
                    );
                }
            }

            let do_full_refresh = match last_full_refresh {
                None => true,
                Some(last) => now - last >= interval_secs,
            };

            if do_full_refresh {
                last_full_refresh = Some(now);

                let accounts_to_refresh: Vec<_> = enabled_accounts
                    .into_iter()
                    .filter(|a| !already_refreshed.contains(&a.id))
                    .collect();

                let total = accounts_to_refresh.len();
                if total == 0 {
                    tracing::debug!(
                        "[QuotaRefresh] All accounts already refreshed, skipping full refresh"
                    );
                } else {
                    tracing::info!(
                        "[QuotaRefresh] Full refresh (interval: {}min, {} accounts)...",
                        interval_minutes,
                        total
                    );

                    let mut success = 0;

                    for mut acc in accounts_to_refresh {
                        match account::fetch_quota_with_retry(&mut acc).await {
                            Ok(_) => {
                                if let Some(quota) = acc.quota.clone() {
                                    persist_quota(&state, &acc.id, &acc.email, quota).await;
                                }
                                success += 1;
                            },
                            Err(e) => {
                                tracing::warn!("[QuotaRefresh] Failed {}: {}", acc.email, e);
                            },
                        }
                        tokio::time::sleep(antigravity_core::proxy::retry::THUNDERING_HERD_DELAY)
                            .await;
                    }

                    tracing::info!("[QuotaRefresh] Full refresh: {}/{} accounts", success, total);
                    if let Err(e) = state.reload_accounts().await {
                        tracing::warn!(
                            "[QuotaRefresh] Failed to reload accounts after full refresh: {}",
                            e
                        );
                    }
                }
            }
        }
    });
}
