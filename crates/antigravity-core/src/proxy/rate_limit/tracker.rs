use dashmap::DashMap;
use std::time::{Duration, SystemTime};

use super::duration_to_secs_ceil;
use super::rate_limit_info::{RateLimitInfo, RateLimitKey};

pub struct RateLimitTracker {
    pub(super) limits: DashMap<RateLimitKey, RateLimitInfo>,
    pub(super) failure_counts: DashMap<RateLimitKey, (u32, SystemTime)>,
}

impl RateLimitTracker {
    pub fn new() -> Self {
        Self {
            limits: DashMap::new(),
            failure_counts: DashMap::new(),
        }
    }

    /// Get remaining wait time in seconds for account
    pub fn get_remaining_wait(&self, account_id: &str) -> u64 {
        let key = RateLimitKey::account(account_id);
        if let Some(info) = self.limits.get(&key) {
            let now = SystemTime::now();
            if info.reset_time > now {
                let duration = info
                    .reset_time
                    .duration_since(now)
                    .unwrap_or(Duration::from_secs(0));
                return duration_to_secs_ceil(duration);
            }
        }
        0
    }

    /// Mark account request success, reset consecutive failure count
    pub fn mark_success(&self, account_id: &str) {
        let key = RateLimitKey::account(account_id);
        if self.failure_counts.remove(&key).is_some() {
            tracing::debug!(
                "account {} request success, reset failure count",
                account_id
            );
        }
        self.limits.remove(&key);
    }

    pub fn get(&self, account_id: &str) -> Option<RateLimitInfo> {
        let key = RateLimitKey::account(account_id);
        self.limits.get(&key).map(|r| r.clone())
    }

    pub fn get_for_model(&self, account_id: &str, model: &str) -> Option<RateLimitInfo> {
        let key = RateLimitKey::model(account_id, model);
        self.limits.get(&key).map(|r| r.clone())
    }

    /// Check if account is still rate limited
    pub fn is_rate_limited(&self, account_id: &str) -> bool {
        if let Some(info) = self.get(account_id) {
            info.reset_time > SystemTime::now()
        } else {
            false
        }
    }

    /// Check if account is rate-limited for specific model.
    /// Checks both account-level AND model-specific limits.
    pub fn is_rate_limited_for_model(&self, account_id: &str, model: &str) -> bool {
        let now = SystemTime::now();

        let account_key = RateLimitKey::account(account_id);
        if let Some(info) = self.limits.get(&account_key) {
            if info.reset_time > now {
                return true;
            }
        }

        let model_key = RateLimitKey::model(account_id, model);
        if let Some(info) = self.limits.get(&model_key) {
            if info.reset_time > now {
                return true;
            }
        }

        false
    }

    pub fn get_remaining_wait_for_model(&self, account_id: &str, model: &str) -> u64 {
        let now = SystemTime::now();
        let mut max_wait: u64 = 0;

        let account_key = RateLimitKey::account(account_id);
        if let Some(info) = self.limits.get(&account_key) {
            if info.reset_time > now {
                let duration = info
                    .reset_time
                    .duration_since(now)
                    .unwrap_or(Duration::from_secs(0));
                max_wait = max_wait.max(duration_to_secs_ceil(duration));
            }
        }

        let model_key = RateLimitKey::model(account_id, model);
        if let Some(info) = self.limits.get(&model_key) {
            if info.reset_time > now {
                let duration = info
                    .reset_time
                    .duration_since(now)
                    .unwrap_or(Duration::from_secs(0));
                max_wait = max_wait.max(duration_to_secs_ceil(duration));
            }
        }

        max_wait
    }

    /// Clear model-specific failure count on success
    pub fn mark_model_success(&self, account_id: &str, model: &str) {
        let key = RateLimitKey::model(account_id, model);
        if self.failure_counts.remove(&key).is_some() {
            tracing::debug!("{}:{} success, reset failure count", account_id, model);
        }
        self.limits.remove(&key);
    }

    /// Get seconds until rate limit reset
    pub fn get_reset_seconds(&self, account_id: &str) -> Option<u64> {
        if let Some(info) = self.get(account_id) {
            info.reset_time
                .duration_since(SystemTime::now())
                .ok()
                .map(|d| d.as_secs())
        } else {
            None
        }
    }

    /// Cleanup expired rate limit records
    #[allow(dead_code)]
    pub fn cleanup_expired(&self) -> usize {
        let now = SystemTime::now();
        let mut count = 0;

        self.limits.retain(|_k, v| {
            if v.reset_time <= now {
                count += 1;
                false
            } else {
                true
            }
        });

        if count > 0 {
            tracing::debug!("Cleared {} expired rate limit records", count);
        }

        count
    }

    /// Clear rate limit record for account
    pub fn clear(&self, account_id: &str) -> bool {
        let key = RateLimitKey::account(account_id);
        self.limits.remove(&key).is_some()
    }

    /// Clear all rate limit records (optimistic reset)
    pub fn clear_all(&self) {
        let count = self.limits.len();
        self.limits.clear();
        tracing::warn!(
            "🔄 Optimistic reset: Cleared all {} rate limit record(s)",
            count
        );
    }
}

impl Default for RateLimitTracker {
    fn default() -> Self {
        Self::new()
    }
}
