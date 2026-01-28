//! Background Schedulers
//!
//! ## Smart Warmup Scheduler
//! Background task that periodically warms up accounts to maintain active sessions.
//!
//! Modes:
//! - `only_low_quota: false` — Warms up accounts with 100% quota to prevent staleness
//! - `only_low_quota: true` — Warms up accounts with <50% quota to refresh them
//!
//! Features:
//! - Configurable interval (default 60 minutes)
//! - 4-hour cooldown per account to prevent re-warming
//! - Whitelisted models only (from SmartWarmupConfig)
//! - Persistent history across restarts (async I/O)
//! - Groups warmup by account to avoid N+1 API calls
//!
//! ## Auto Quota Refresh Scheduler
//! Background task that periodically refreshes account quotas from Google API.
//!
//! Features:
//! - Enabled via `config.auto_refresh` flag
//! - Configurable interval via `config.refresh_interval` (minutes, default 15)
//! - Required for quota protection and smart warmup to have fresh data

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

use crate::state::AppState;
use antigravity_core::modules::{account, config};

/// Cooldown period: 4 hours (matches Pro account 5h reset, leaving 1h margin)
const COOLDOWN_SECONDS: i64 = 14400;

/// Threshold for low quota mode
const LOW_QUOTA_THRESHOLD: i32 = 50;

/// Default models to warmup if config is empty
const DEFAULT_WARMUP_MODELS: &[&str] = &[
    "gemini-3-flash",
    "claude-sonnet-4-5",
    "gemini-3-pro-high",
    "gemini-3-pro-image",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WarmupHistory {
    entries: HashMap<String, i64>,
}

struct SchedulerState {
    history: WarmupHistory,
    history_path: PathBuf,
}

impl SchedulerState {
    async fn new_async(data_dir: PathBuf) -> Self {
        let history_path = data_dir.join("warmup_history.json");
        let history = Self::load_history_async(&history_path).await;
        Self {
            history,
            history_path,
        }
    }

    async fn load_history_async(path: &PathBuf) -> WarmupHistory {
        if path.exists() {
            match tokio::fs::read_to_string(path).await {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("[Scheduler] Failed to read history file: {}", e);
                    WarmupHistory::default()
                }
            }
        } else {
            WarmupHistory::default()
        }
    }

    async fn save_history_async(&self) {
        let path = self.history_path.clone();
        let content = match serde_json::to_string_pretty(&self.history) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[Scheduler] Failed to serialize history: {}", e);
                return;
            }
        };
        if let Err(e) = tokio::fs::write(&path, content).await {
            tracing::warn!("[Scheduler] Failed to write history to {:?}: {}", path, e);
        }
    }

    fn record_warmup(&mut self, key: &str, timestamp: i64) {
        self.history.entries.insert(key.to_string(), timestamp);
    }

    fn is_in_cooldown(&self, key: &str, now: i64) -> bool {
        self.history
            .entries
            .get(key)
            .is_some_and(|&ts| now - ts < COOLDOWN_SECONDS)
    }

    fn cleanup_stale(&mut self, cutoff: i64) -> usize {
        let before = self.history.entries.len();
        self.history.entries.retain(|_, &mut ts| ts > cutoff);
        before - self.history.entries.len()
    }
}

/// Start the smart warmup scheduler as a background tokio task
pub fn start(state: AppState) {
    tokio::spawn(async move {
        // Get data directory for history persistence
        let data_dir = match account::get_data_dir() {
            Ok(dir) => dir,
            Err(e) => {
                tracing::error!("❌ [Scheduler] Failed to get data dir: {}", e);
                return;
            }
        };

        let scheduler_state = Arc::new(Mutex::new(SchedulerState::new_async(data_dir).await));

        tracing::info!("🔥 [Scheduler] Smart Warmup Scheduler started");

        // Check config every 60 seconds, run warmup based on interval_minutes
        let mut check_interval = interval(Duration::from_secs(60));
        let mut last_warmup_check: Option<i64> = None;

        loop {
            check_interval.tick().await;

            // Load fresh config each cycle
            let app_config = match config::load_config() {
                Ok(cfg) => cfg,
                Err(_) => continue,
            };

            let warmup_config = &app_config.smart_warmup;

            // Skip if disabled
            if !warmup_config.enabled {
                continue;
            }

            // Check if enough time has passed since last warmup
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
            let interval_secs = (interval_minutes as i64) * 60;

            if let Some(last) = last_warmup_check {
                if now - last < interval_secs {
                    continue;
                }
            }

            // Time to run warmup scan
            last_warmup_check = Some(now);

            tracing::info!("[Scheduler] 🔍 Starting warmup scan...");

            // Get models to monitor
            let models_to_warmup: Vec<String> = if warmup_config.models.is_empty() {
                DEFAULT_WARMUP_MODELS
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                warmup_config.models.clone()
            };

            // Get all accounts
            let accounts = match account::list_accounts() {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!("[Scheduler] Failed to list accounts: {}", e);
                    continue;
                }
            };

            if accounts.is_empty() {
                continue;
            }

            let mode_desc = if warmup_config.only_low_quota {
                "low quota"
            } else {
                "100% quota"
            };
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
                            model.percentage == 100
                        };

                        if should_warmup {
                            if scheduler.is_in_cooldown(&acc.email, now) {
                                skipped_cooldown += 1;
                                continue;
                            }
                            accounts_to_warmup.insert(acc.email.clone());
                            tracing::info!(
                                "[Scheduler] ✓ Account {} has {} at {}%",
                                acc.email,
                                model.name,
                                model.percentage
                            );
                        }
                    }
                }
            }

            {
                let mut scheduler = scheduler_state.lock().await;
                let cutoff = now - 86400;
                let cleaned = scheduler.cleanup_stale(cutoff);
                if cleaned > 0 {
                    tracing::debug!("[Scheduler] Cleaned up {} stale history entries", cleaned);
                }
                scheduler.save_history_async().await;
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

                tracing::info!("[Scheduler] 🔥 Triggering {} account warmups...", total);

                let mut success = 0;

                for email in &accounts_to_warmup {
                    let mut acc = match accounts.iter().find(|a| &a.email == email).cloned() {
                        Some(a) => a,
                        None => continue,
                    };

                    tracing::info!("[Warmup] Refreshing {}", email);

                    match account::fetch_quota_with_retry(&mut acc).await {
                        Ok(_) => {
                            if let Some(quota) = acc.quota.clone() {
                                if let Err(e) = account::update_account_quota(&acc.id, quota) {
                                    tracing::warn!(
                                        "[Warmup] Failed to update quota for {}: {}",
                                        email,
                                        e
                                    );
                                }
                            }
                            success += 1;
                            let mut scheduler = scheduler_state.lock().await;
                            scheduler.record_warmup(email, now);
                        }
                        Err(e) => {
                            tracing::warn!("[Scheduler] Warmup failed for {}: {}", email, e);
                        }
                    }

                    tokio::time::sleep(Duration::from_millis(500)).await;
                }

                {
                    let scheduler = scheduler_state.lock().await;
                    scheduler.save_history_async().await;
                }

                tracing::info!(
                    "[Scheduler] ✅ Warmup completed: {}/{} successful",
                    success,
                    total
                );

                tokio::time::sleep(Duration::from_secs(2)).await;
                let _ = state.reload_accounts().await;
            } else if skipped_cooldown > 0 {
                tracing::info!(
                    "[Scheduler] Scan complete, all {} in cooldown",
                    skipped_cooldown
                );
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

/// Start the auto quota refresh scheduler as a background tokio task
pub fn start_quota_refresh(state: AppState) {
    tokio::spawn(async move {
        tracing::info!("📊 [QuotaRefresh] Auto Quota Refresh Scheduler started");

        let mut check_interval = interval(Duration::from_secs(60));
        let mut last_refresh: Option<i64> = None;

        loop {
            check_interval.tick().await;

            let app_config = match config::load_config() {
                Ok(cfg) => cfg,
                Err(_) => continue,
            };

            if !app_config.auto_refresh {
                continue;
            }

            let now = Utc::now().timestamp();
            let interval_minutes = if app_config.refresh_interval < 5 {
                tracing::warn!(
                    "[QuotaRefresh] refresh_interval {} too low, using 15",
                    app_config.refresh_interval
                );
                15
            } else {
                app_config.refresh_interval
            };
            let interval_secs = (interval_minutes as i64) * 60;

            if let Some(last) = last_refresh {
                if now - last < interval_secs {
                    continue;
                }
            }

            last_refresh = Some(now);

            tracing::info!(
                "[QuotaRefresh] 🔄 Refreshing all account quotas (interval: {}min)...",
                interval_minutes
            );

            let accounts = match account::list_accounts() {
                Ok(accs) => accs,
                Err(e) => {
                    tracing::warn!("[QuotaRefresh] Failed to list accounts: {}", e);
                    continue;
                }
            };
            let enabled_accounts: Vec<_> = accounts
                .into_iter()
                .filter(|a| !a.disabled && !a.proxy_disabled)
                .collect();
            let total = enabled_accounts.len();
            let mut success = 0;

            for mut acc in enabled_accounts {
                match account::fetch_quota_with_retry(&mut acc).await {
                    Ok(_) => {
                        if let Some(quota) = acc.quota.clone() {
                            if let Err(e) = account::update_account_quota(&acc.id, quota) {
                                tracing::warn!(
                                    "[QuotaRefresh] Failed to update quota for {}: {}",
                                    acc.email,
                                    e
                                );
                            }
                        }
                        success += 1;
                    }
                    Err(e) => {
                        tracing::warn!("[QuotaRefresh] Failed to refresh {}: {}", acc.email, e);
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            tracing::info!(
                "[QuotaRefresh] ✅ Refreshed {}/{} accounts successfully",
                success,
                total
            );

            let _ = state.reload_accounts().await;
        }
    });
}
