use std::time::{Duration, SystemTime};

use super::rate_limit_info::{RateLimitInfo, RateLimitKey, RateLimitReason};
use super::tracker::RateLimitTracker;
use super::FAILURE_COUNT_EXPIRY_SECONDS;

impl RateLimitTracker {
    /// Set adaptive temporary lockout based on consecutive failure count.
    /// Returns the lockout duration in seconds.
    /// Progression: 5s → 15s → 30s → 60s (max)
    pub fn set_adaptive_temporary_lockout(&self, account_id: &str) -> u64 {
        let now = SystemTime::now();
        let key = RateLimitKey::account(account_id);

        let failure_count = {
            let mut entry = self.failure_counts.entry(key.clone()).or_insert((0, now));

            let elapsed = now.duration_since(entry.1).unwrap_or(Duration::from_secs(0)).as_secs();
            if elapsed > FAILURE_COUNT_EXPIRY_SECONDS {
                *entry = (0, now);
            }

            entry.0 += 1;
            entry.1 = now;
            entry.0
        };

        let lockout_secs = match failure_count {
            1 => 5,
            2 => 15,
            3 => 30,
            _ => {
                tracing::warn!(
                    "Account {} hit max lockout (attempt #{}), persistent failures detected",
                    account_id,
                    failure_count
                );
                60
            },
        };

        let info = RateLimitInfo {
            reset_time: now + Duration::from_secs(lockout_secs),
            retry_after_sec: lockout_secs,
            detected_at: now,
            reason: RateLimitReason::Unknown,
            model: None,
        };

        self.limits.insert(key, info);

        tracing::debug!(
            "⚡ Account {} adaptive lockout: {}s (attempt #{})",
            account_id,
            lockout_secs,
            failure_count
        );

        lockout_secs
    }

    /// Lock account until exact reset time
    pub fn set_lockout_until(
        &self,
        account_id: &str,
        reset_time: SystemTime,
        reason: RateLimitReason,
        model: Option<String>,
    ) {
        let now = SystemTime::now();
        let retry_sec = reset_time.duration_since(now).map(|d| d.as_secs()).unwrap_or(60);

        let info = RateLimitInfo {
            reset_time,
            retry_after_sec: retry_sec,
            detected_at: now,
            reason,
            model: model.clone(),
        };

        let key = RateLimitKey::from_optional_model(account_id, model.as_deref());
        self.limits.insert(key, info);

        if let Some(m) = &model {
            tracing::info!(
                "account {} model {} locked until quota refresh time, {} seconds remaining",
                account_id,
                m,
                retry_sec
            );
        } else {
            tracing::info!(
                "account {} locked until quota refresh time, {} seconds remaining",
                account_id,
                retry_sec
            );
        }
    }

    /// Lock account using ISO 8601 time string
    pub fn set_lockout_until_iso(
        &self,
        account_id: &str,
        reset_time_str: &str,
        reason: RateLimitReason,
        model: Option<String>,
    ) -> bool {
        match chrono::DateTime::parse_from_rfc3339(reset_time_str) {
            Ok(dt) => {
                let ts = dt.timestamp();
                if ts < 0 {
                    tracing::warn!("quotarefreshtime '{}' at 1970 before，ignore", reset_time_str);
                    return false;
                }
                let reset_time = SystemTime::UNIX_EPOCH + Duration::from_secs(ts as u64);
                self.set_lockout_until(account_id, reset_time, reason, model);
                true
            },
            Err(e) => {
                tracing::warn!(
                    "Cannot parse quota refresh time '{}': {}, will use default backoff strategy",
                    reset_time_str,
                    e
                );
                false
            },
        }
    }

    /// Set lockout for specific account:model pair
    pub fn set_model_lockout(
        &self,
        account_id: &str,
        model: &str,
        reset_time: SystemTime,
        reason: RateLimitReason,
    ) {
        let now = SystemTime::now();
        let retry_sec = reset_time.duration_since(now).map(|d| d.as_secs()).unwrap_or(60);

        let key = RateLimitKey::model(account_id, model);
        let info = RateLimitInfo {
            reset_time,
            retry_after_sec: retry_sec,
            detected_at: now,
            reason,
            model: Some(model.to_string()),
        };

        self.limits.insert(key, info);
        tracing::info!(
            "🔒 Account {}:{} locked for {}s ({:?})",
            account_id,
            model,
            retry_sec,
            reason
        );
    }

    /// Adaptive temporary lockout for specific model.
    /// Returns lockout duration. Progression: 5s → 15s → 30s → 60s
    pub fn set_adaptive_model_lockout(&self, account_id: &str, model: &str) -> u64 {
        let now = SystemTime::now();
        let key = RateLimitKey::model(account_id, model);

        let failure_count = {
            let mut entry = self.failure_counts.entry(key.clone()).or_insert((0, now));

            let elapsed = now.duration_since(entry.1).unwrap_or(Duration::from_secs(0)).as_secs();
            if elapsed > FAILURE_COUNT_EXPIRY_SECONDS {
                *entry = (0, now);
            }

            entry.0 += 1;
            entry.1 = now;
            entry.0
        };

        let lockout_secs = match failure_count {
            1 => 5,
            2 => 15,
            3 => 30,
            _ => {
                tracing::warn!(
                    "{}:{} hit max lockout (attempt #{}), persistent failures detected",
                    account_id,
                    model,
                    failure_count
                );
                60
            },
        };

        let info = RateLimitInfo {
            reset_time: now + Duration::from_secs(lockout_secs),
            retry_after_sec: lockout_secs,
            detected_at: now,
            reason: RateLimitReason::RateLimitExceeded,
            model: Some(model.to_string()),
        };

        self.limits.insert(key, info);

        tracing::debug!(
            "⚡ {}:{} adaptive lockout: {}s (attempt #{})",
            account_id,
            model,
            lockout_secs,
            failure_count
        );

        lockout_secs
    }
}
