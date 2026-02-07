use crate::proxy::rate_limit::RateLimitTracker;
use crate::proxy::routing_config::SmartRoutingConfig;
use crate::proxy::AdaptiveLimitManager;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

mod file_utils;
mod health;
mod persistence;
mod proxy_token;
mod rate_limiter;
mod recovery;
mod routing;
mod selection;
mod selection_helpers;
mod session;
mod store;
mod token_refresh;

pub use proxy_token::{AccountTier, ProxyToken};

pub struct TokenManager {
    pub(crate) tokens: Arc<DashMap<String, ProxyToken>>,
    pub(crate) data_dir: PathBuf,
    pub(crate) rate_limit_tracker: Arc<RateLimitTracker>,
    pub(crate) routing_config: Arc<tokio::sync::RwLock<SmartRoutingConfig>>,
    pub(crate) session_accounts: Arc<DashMap<String, String>>,
    pub(crate) adaptive_limits: Arc<tokio::sync::RwLock<Option<Arc<AdaptiveLimitManager>>>>,
    pub(crate) preferred_account_id: Arc<tokio::sync::RwLock<Option<String>>>,
    pub(crate) health_scores: Arc<DashMap<String, f32>>,
    pub(crate) active_requests: Arc<DashMap<String, AtomicU32>>,
    pub(crate) session_failures: Arc<DashMap<String, AtomicU32>>,
    pub(crate) file_locks: Arc<DashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl TokenManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            tokens: Arc::new(DashMap::new()),
            data_dir,
            rate_limit_tracker: Arc::new(RateLimitTracker::new()),
            routing_config: Arc::new(tokio::sync::RwLock::new(SmartRoutingConfig::default())),
            session_accounts: Arc::new(DashMap::new()),
            adaptive_limits: Arc::new(tokio::sync::RwLock::new(None)),
            preferred_account_id: Arc::new(tokio::sync::RwLock::new(None)),
            health_scores: Arc::new(DashMap::new()),
            active_requests: Arc::new(DashMap::new()),
            session_failures: Arc::new(DashMap::new()),
            file_locks: Arc::new(DashMap::new()),
        }
    }

    pub fn increment_active_requests(&self, email: &str) -> u32 {
        self.active_requests
            .entry(email.to_string())
            .or_insert_with(|| AtomicU32::new(0))
            .fetch_add(1, Ordering::AcqRel)
            + 1
    }

    pub fn decrement_active_requests(&self, email: &str) {
        if let Some(counter) = self.active_requests.get(email) {
            let _ = counter.fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                if v > 0 {
                    Some(v - 1)
                } else {
                    None
                }
            });
        }
    }

    pub fn get_active_requests(&self, email: &str) -> u32 {
        self.active_requests.get(email).map(|c| c.load(Ordering::Acquire)).unwrap_or(0)
    }

    pub async fn set_adaptive_limits(&self, tracker: Arc<AdaptiveLimitManager>) {
        let mut guard = self.adaptive_limits.write().await;
        *guard = Some(tracker);
    }

    pub fn start_auto_cleanup(&self) {
        let tracker = self.rate_limit_tracker.clone();
        let session_failures = self.session_failures.clone();
        let session_accounts = self.session_accounts.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let cleaned = tracker.cleanup_expired();
                if cleaned > 0 {
                    tracing::info!(
                        "Auto-cleanup: Removed {} expired rate limit record(s)",
                        cleaned
                    );
                }
                let before = session_failures.len();
                session_failures.retain(|_, v| v.load(Ordering::Relaxed) > 0);
                let cleaned_sessions = before - session_failures.len();
                if cleaned_sessions > 0 {
                    tracing::debug!("Cleaned {} stale session failure record(s)", cleaned_sessions);
                }
                // Clean stale session bindings (keep max 10000)
                let session_count = session_accounts.len();
                if session_count > 10_000 {
                    let to_remove = session_count - 5_000;
                    let keys_to_remove: Vec<String> = session_accounts
                        .iter()
                        .take(to_remove)
                        .map(|entry| entry.key().clone())
                        .collect();
                    for key in &keys_to_remove {
                        session_accounts.remove(key);
                    }
                    tracing::info!(
                        "Session cleanup: removed {} stale session bindings ({} -> {})",
                        keys_to_remove.len(),
                        session_count,
                        session_accounts.len()
                    );
                }
            }
        });
        tracing::info!("Rate limit auto-cleanup task started (interval: 60s)");
    }

    pub fn start_auto_account_sync(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            interval.tick().await;

            loop {
                interval.tick().await;
                match manager.reload_all_accounts().await {
                    Ok(count) => {
                        tracing::debug!("Auto-sync: Reloaded {} account(s) from disk", count);
                    },
                    Err(e) => {
                        tracing::warn!("Auto-sync: Failed to reload accounts: {}", e);
                    },
                }
            }
        });
        tracing::info!("Account auto-sync task started (interval: 60s)");
    }

    pub fn get_all_available_models(&self) -> Vec<String> {
        use std::collections::HashSet;
        let mut models: HashSet<String> = HashSet::new();
        for entry in self.tokens.iter() {
            for model in &entry.value().available_models {
                let _: bool = models.insert(model.clone());
            }
        }
        let mut sorted: Vec<String> = models.into_iter().collect();
        sorted.sort();
        sorted
    }
}

#[cfg(test)]
mod tests;
