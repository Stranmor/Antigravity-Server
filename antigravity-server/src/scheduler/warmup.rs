use chrono::Utc;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::interval;

use crate::state::AppState;
use antigravity_core::modules::{account, config};

use super::state::{SchedulerState, DEFAULT_WARMUP_MODELS, LOW_QUOTA_THRESHOLD};

/// Start the smart warmup scheduler as a background tokio task
pub fn start(state: AppState) {
    tokio::spawn(async move {
        let data_dir = match tokio::task::spawn_blocking(account::get_data_dir).await {
            Ok(res) => match res {
                Ok(dir) => dir,
                Err(e) => {
                    tracing::error!("[Scheduler] Failed to get data dir: {}", e);
                    return;
                },
            },
            Err(e) => {
                tracing::error!("[Scheduler] spawn_blocking panic for get_data_dir: {}", e);
                return;
            },
        };

        let scheduler_state = Arc::new(Mutex::new(SchedulerState::new_async(data_dir).await));

        tracing::info!("[Scheduler] Smart Warmup Scheduler started");

        let mut check_interval = interval(Duration::from_secs(60));
        let mut last_warmup_check: Option<i64> = None;

        loop {
            check_interval.tick().await;

            let app_config = match tokio::task::spawn_blocking(config::load_config).await {
                Ok(res) => match res {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        tracing::error!("[Scheduler] Failed to load config: {}", e);
                        continue;
                    },
                },
                Err(e) => {
                    tracing::error!("[Scheduler] spawn_blocking panic for load_config: {}", e);
                    continue;
                },
            };

            let warmup_config = &app_config.smart_warmup;

            if !warmup_config.enabled {
                continue;
            }

            let now = Utc::now().timestamp();
            let interval_minutes = if warmup_config.interval_minutes < 5 {
                tracing::warn!(
                    "[Scheduler] interval_minutes {} too low, using 60",
                    warmup_config.interval_minutes
                );
                60
            } else {
                warmup_config.interval_minutes
            };
            let interval_secs = i64::from(interval_minutes) * 60_i64;

            if let Some(last) = last_warmup_check {
                if now - last < interval_secs {
                    continue;
                }
            }

            last_warmup_check = Some(now);

            tracing::info!("[Scheduler] Starting warmup scan...");

            let models_to_warmup: Vec<String> = if warmup_config.models.is_empty() {
                DEFAULT_WARMUP_MODELS.iter().map(|s| s.to_string()).collect()
            } else {
                warmup_config.models.clone()
            };

            let accounts = match tokio::task::spawn_blocking(account::list_accounts).await {
                Ok(res) => match res {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::warn!("[Scheduler] Failed to list accounts: {}", e);
                        continue;
                    },
                },
                Err(e) => {
                    tracing::error!("[Scheduler] spawn_blocking panic for list_accounts: {}", e);
                    continue;
                },
            };

            if accounts.is_empty() {
                continue;
            }

            let mode_desc = if warmup_config.only_low_quota { "low quota" } else { "100% quota" };
            tracing::info!(
                "[Scheduler] Scanning {} accounts for {} models...",
                accounts.len(),
                mode_desc
            );

            let mut accounts_to_warmup: HashSet<String> = HashSet::new();
            let mut skipped_cooldown = 0;
            let mut skipped_disabled = 0;

            {
                let scheduler = scheduler_state.lock().await;

                for acc in &accounts {
                    if acc.disabled || acc.proxy_disabled {
                        skipped_disabled += 1;
                        continue;
                    }

                    let quota = match &acc.quota {
                        Some(q) => q,
                        None => continue,
                    };

                    for model in &quota.models {
                        let model_matches = models_to_warmup
                            .iter()
                            .any(|m| model.name.to_lowercase().contains(&m.to_lowercase()));

                        if !model_matches {
                            continue;
                        }

                        let should_warmup = if warmup_config.only_low_quota {
                            model.percentage < LOW_QUOTA_THRESHOLD
                        } else {
                            model.percentage == 100_i32
                        };

                        if should_warmup {
                            if scheduler.is_in_cooldown(&acc.email, now) {
                                skipped_cooldown += 1;
                                continue;
                            }
                            accounts_to_warmup.insert(acc.email.clone());
                            tracing::info!(
                                "[Scheduler] Account {} has {} at {}%",
                                acc.email,
                                model.name,
                                model.percentage
                            );
                        }
                    }
                }
                drop(scheduler);
            }

            {
                let mut scheduler = scheduler_state.lock().await;
                let cutoff = now - 86400_i64;
                let cleaned = scheduler.cleanup_stale(cutoff);
                if cleaned > 0 {
                    tracing::debug!("[Scheduler] Cleaned up {} stale history entries", cleaned);
                }
                scheduler.save_history_async().await;
                drop(scheduler);
            }

            if !accounts_to_warmup.is_empty() {
                let total = accounts_to_warmup.len();

                if skipped_cooldown > 0 {
                    tracing::info!(
                        "[Scheduler] Skipped {} in cooldown, warming {} accounts",
                        skipped_cooldown,
                        total
                    );
                }

                tracing::info!("[Scheduler] Triggering {} account warmups...", total);

                let mut success = 0_usize;

                // Convert HashSet to Vec for deterministic iteration
                let emails_to_warmup: Vec<_> = accounts_to_warmup.into_iter().collect();
                for email in &emails_to_warmup {
                    let mut acc = match accounts.iter().find(|a| &a.email == email).cloned() {
                        Some(a) => a,
                        None => continue,
                    };

                    tracing::info!("[Warmup] Refreshing {}", email);

                    match account::fetch_quota_with_retry(&mut acc).await {
                        Ok(_) => {
                            if let Some(quota) = acc.quota.clone() {
                                let protected_models = match account::update_account_quota_async(
                                    acc.id.clone(),
                                    quota.clone(),
                                )
                                .await
                                {
                                    Ok(updated) => {
                                        Some(updated.protected_models.iter().cloned().collect())
                                    },
                                    Err(e) => {
                                        tracing::warn!(
                                            "[Warmup] Failed to update quota for {}: {}",
                                            email,
                                            e
                                        );
                                        None
                                    },
                                };
                                if let Some(repo) = state.repository() {
                                    match repo.get_account_by_email(&acc.email).await {
                                        Ok(Some(pg_account)) => {
                                            if let Err(e) = repo
                                                .update_quota(
                                                    &pg_account.id,
                                                    quota,
                                                    protected_models,
                                                )
                                                .await
                                            {
                                                tracing::warn!(
                                                    "[Warmup] DB quota update failed for {}: {}",
                                                    email,
                                                    e
                                                );
                                            }
                                        },
                                        Ok(None) => {
                                            tracing::warn!(
                                                "[Warmup] PG account lookup failed for {}",
                                                email
                                            );
                                        },
                                        Err(e) => {
                                            tracing::warn!(
                                                "[Warmup] PG account lookup error for {}: {}",
                                                email,
                                                e
                                            );
                                        },
                                    }
                                }
                            }
                            success += 1;
                            let mut scheduler = scheduler_state.lock().await;
                            scheduler.record_warmup(email, now);
                        },
                        Err(e) => {
                            tracing::warn!("[Scheduler] Warmup failed for {}: {}", email, e);
                        },
                    }

                    tokio::time::sleep(antigravity_core::proxy::retry::THUNDERING_HERD_DELAY).await;
                }

                {
                    let scheduler = scheduler_state.lock().await;
                    scheduler.save_history_async().await;
                    drop(scheduler);
                }

                tracing::info!("[Scheduler] Warmup completed: {}/{} successful", success, total);

                tokio::time::sleep(Duration::from_secs(2)).await;
                drop(state.reload_accounts().await);
            } else if skipped_cooldown > 0 {
                tracing::info!("[Scheduler] Scan complete, all {} in cooldown", skipped_cooldown);
            } else if skipped_disabled > 0 {
                tracing::debug!(
                    "[Scheduler] Scan complete, {} disabled, no matching models",
                    skipped_disabled
                );
            } else {
                tracing::debug!("[Scheduler] Scan complete, no accounts need warmup");
            }
        }
    });
}
