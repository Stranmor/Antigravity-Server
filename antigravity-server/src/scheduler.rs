//! Smart Warmup Scheduler
//!
//! Background task that periodically warms up accounts with 100% quota.
//! Prevents accounts from going stale and ensures quota is actively used.
//!
//! Features:
//! - Configurable interval (default 60 minutes)
//! - 4-hour cooldown per model to prevent re-warming
//! - Whitelisted models only (from SmartWarmupConfig)
//! - Persistent history across restarts

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

use crate::state::AppState;
use antigravity_core::modules::{account, config};

/// Cooldown period: 4 hours (matches Pro account 5h reset, leaving 1h margin)
const COOLDOWN_SECONDS: i64 = 14400;

/// Default models to warmup if config is empty
const DEFAULT_WARMUP_MODELS: &[&str] = &[
    "gemini-3-flash",
    "claude-sonnet-4-5",
    "gemini-3-pro-high",
    "gemini-3-pro-image",
];

/// Warmup history entry
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct WarmupHistory {
    /// Key = "email:model:100", Value = timestamp when warmed up
    entries: HashMap<String, i64>,
}

/// Scheduler state
struct SchedulerState {
    history: WarmupHistory,
    history_path: PathBuf,
}

impl SchedulerState {
    fn new(data_dir: PathBuf) -> Self {
        let history_path = data_dir.join("warmup_history.json");
        let history = Self::load_history(&history_path);
        Self {
            history,
            history_path,
        }
    }

    fn load_history(path: &PathBuf) -> WarmupHistory {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => WarmupHistory::default(),
            }
        } else {
            WarmupHistory::default()
        }
    }

    fn save_history(&self) {
        if let Ok(content) = serde_json::to_string_pretty(&self.history) {
            let _ = std::fs::write(&self.history_path, content);
        }
    }

    fn record_warmup(&mut self, key: &str, timestamp: i64) {
        self.history.entries.insert(key.to_string(), timestamp);
        self.save_history();
    }

    fn is_in_cooldown(&self, key: &str, now: i64) -> bool {
        if let Some(&last_ts) = self.history.entries.get(key) {
            now - last_ts < COOLDOWN_SECONDS
        } else {
            false
        }
    }

    fn clear_entry(&mut self, key: &str) {
        if self.history.entries.remove(key).is_some() {
            self.save_history();
        }
    }

    /// Clean entries older than 24 hours
    fn cleanup_old_entries(&mut self) {
        let now = Utc::now().timestamp();
        let cutoff = now - 86400; // 24 hours
        let before = self.history.entries.len();
        self.history.entries.retain(|_, &mut ts| ts > cutoff);
        if self.history.entries.len() < before {
            self.save_history();
        }
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

        let scheduler_state = Arc::new(Mutex::new(SchedulerState::new(data_dir)));

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
            let interval_secs = (warmup_config.interval_minutes as i64) * 60;

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

            tracing::info!(
                "[Scheduler] Scanning {} accounts for 100% quota models...",
                accounts.len()
            );

            let mut warmup_tasks: Vec<(String, String)> = Vec::new(); // (email, model)
            let mut skipped_cooldown = 0;
            let mut skipped_disabled = 0;

            for acc in &accounts {
                // Skip disabled accounts
                if acc.disabled || acc.proxy_disabled {
                    skipped_disabled += 1;
                    continue;
                }

                // Get quota data
                let quota = match &acc.quota {
                    Some(q) => q,
                    None => continue,
                };

                let mut scheduler = scheduler_state.lock().await;

                for model in &quota.models {
                    // Check if this model is in our warmup list
                    let model_matches = models_to_warmup
                        .iter()
                        .any(|m| model.name.to_lowercase().contains(&m.to_lowercase()));

                    if !model_matches {
                        continue;
                    }

                    let history_key = format!("{}:{}:100", acc.email, model.name);

                    if model.percentage == 100 {
                        // Check cooldown
                        if scheduler.is_in_cooldown(&history_key, now) {
                            skipped_cooldown += 1;
                            continue;
                        }

                        warmup_tasks.push((acc.email.clone(), model.name.clone()));

                        tracing::info!(
                            "[Scheduler] ✓ Scheduled warmup: {} @ {} (quota at 100%)",
                            model.name,
                            acc.email
                        );
                    } else if model.percentage < 100 {
                        // Quota not full, clear history entry to allow warmup when it resets
                        scheduler.clear_entry(&history_key);
                    }
                }

                // Cleanup old entries periodically
                scheduler.cleanup_old_entries();
            }

            // Execute warmup tasks
            if !warmup_tasks.is_empty() {
                let total = warmup_tasks.len();

                if skipped_cooldown > 0 {
                    tracing::info!(
                        "[Scheduler] Skipped {} models in cooldown, warming {} models",
                        skipped_cooldown,
                        total
                    );
                }

                tracing::info!("[Scheduler] 🔥 Triggering {} warmup tasks...", total);

                let mut success = 0;

                // Process in batches of 3 to avoid overwhelming the API
                for (batch_idx, batch) in warmup_tasks.chunks(3).enumerate() {
                    let mut handles = Vec::new();

                    for (task_idx, (email, model)) in batch.iter().enumerate() {
                        let global_idx = batch_idx * 3 + task_idx + 1;
                        let email = email.clone();
                        let model = model.clone();
                        let email_for_handle = email.clone();
                        let model_for_handle = model.clone();

                        tracing::info!("[Warmup {}/{}] {} @ {}", global_idx, total, model, email);

                        // Spawn warmup task
                        let handle = tokio::spawn(async move {
                            // Find the account and refresh its quota (this triggers warmup)
                            let accounts = account::list_accounts().ok()?;
                            let mut acc = accounts.into_iter().find(|a| a.email == email)?;

                            // Refresh quota - this makes an API call which "warms up" the account
                            match account::fetch_quota_with_retry(&mut acc).await {
                                Ok(_) => {
                                    let _ = account::save_account(&acc);
                                    Some(true)
                                }
                                Err(_) => Some(false),
                            }
                        });

                        handles.push((handle, email_for_handle, model_for_handle));
                    }

                    // Wait for batch to complete
                    for (handle, email, model) in handles {
                        if let Ok(Some(true)) = handle.await {
                            success += 1;
                            let history_key = format!("{}:{}:100", email, model);
                            let mut scheduler = scheduler_state.lock().await;
                            scheduler.record_warmup(&history_key, now);
                        }
                    }

                    // Small delay between batches
                    if batch_idx < warmup_tasks.len().div_ceil(3) - 1 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }

                tracing::info!(
                    "[Scheduler] ✅ Warmup completed: {}/{} successful",
                    success,
                    total
                );

                // Reload accounts into token manager
                tokio::time::sleep(Duration::from_secs(2)).await;
                let _ = state.reload_accounts().await;
            } else if skipped_cooldown > 0 {
                tracing::info!(
                    "[Scheduler] Scan complete, all {} models in cooldown period",
                    skipped_cooldown
                );
            } else if skipped_disabled > 0 {
                tracing::debug!(
                    "[Scheduler] Scan complete, {} accounts disabled, no 100% quota models found",
                    skipped_disabled
                );
            } else {
                tracing::debug!("[Scheduler] Scan complete, no 100% quota models need warmup");
            }
        }
    });
}
