use super::TokenManager;
use crate::modules::quota;
use crate::proxy::rate_limit::RateLimitReason;

impl TokenManager {
    pub fn mark_rate_limited(
        &self,
        account_id: &str,
        status: u16,
        retry_after_header: Option<&str>,
        error_body: &str,
    ) {
        self.mark_rate_limited_with_model(account_id, status, retry_after_header, error_body, None);
    }

    pub fn mark_rate_limited_with_model(
        &self,
        account_id: &str,
        status: u16,
        retry_after_header: Option<&str>,
        error_body: &str,
        model: Option<String>,
    ) {
        self.rate_limit_tracker.parse_from_error(
            account_id,
            status,
            retry_after_header,
            error_body,
            model,
        );
    }

    pub fn is_rate_limited(&self, account_id: &str) -> bool {
        self.rate_limit_tracker.is_rate_limited(account_id)
    }

    pub fn is_rate_limited_for_model(&self, account_id: &str, model: &str) -> bool {
        self.rate_limit_tracker.is_rate_limited_for_model(account_id, model)
    }

    pub fn rate_limit_tracker(&self) -> &crate::proxy::rate_limit::RateLimitTracker {
        &self.rate_limit_tracker
    }

    #[allow(dead_code, reason = "public API used in tests")]
    pub fn get_rate_limit_reset_seconds(&self, account_id: &str) -> Option<u64> {
        self.rate_limit_tracker.get_reset_seconds(account_id)
    }

    #[allow(dead_code, reason = "public API used in tests")]
    pub fn cleanup_expired_rate_limits(&self) -> usize {
        self.rate_limit_tracker.cleanup_expired()
    }

    pub fn clear_rate_limit(&self, account_id: &str) -> bool {
        self.rate_limit_tracker.clear(account_id)
    }

    pub fn clear_all_rate_limits(&self) {
        self.rate_limit_tracker.clear_all();
    }

    pub async fn get_quota_reset_time(&self, email: &str) -> Option<String> {
        let account_path = self
            .tokens
            .iter()
            .find(|entry| entry.value().email == email)
            .map(|entry| entry.value().account_path.clone())?;

        let content = tokio::fs::read_to_string(&account_path).await.ok()?;
        let account: serde_json::Value = serde_json::from_str(&content).ok()?;

        let models =
            account.get("quota").and_then(|q| q.get("models")).and_then(|m| m.as_array())?;

        models
            .iter()
            .filter_map(|model| model.get("reset_time").and_then(|r| r.as_str()))
            .filter(|s| !s.is_empty())
            .min()
            .map(|s| s.to_string())
    }

    pub async fn set_precise_lockout(
        &self,
        email: &str,
        reason: RateLimitReason,
        model: Option<String>,
    ) -> bool {
        if let Some(reset_time_str) = self.get_quota_reset_time(email).await {
            tracing::info!("Found quota reset time for account {}: {}", email, reset_time_str);
            self.rate_limit_tracker.set_lockout_until_iso(email, &reset_time_str, reason, model)
        } else {
            tracing::debug!(
                "No quota reset time found for account {}, using default backoff",
                email
            );
            false
        }
    }

    pub async fn fetch_and_lock_with_realtime_quota(
        &self,
        email: &str,
        reason: RateLimitReason,
        model: Option<String>,
    ) -> bool {
        let access_token = {
            let mut found_token: Option<String> = None;
            for entry in self.tokens.iter() {
                if entry.value().email == email {
                    found_token = Some(entry.value().access_token.clone());
                    break;
                }
            }
            found_token
        };

        let access_token = match access_token {
            Some(t) => t,
            None => {
                tracing::warn!(
                    "Cannot find access_token for account {}, cannot refresh quota",
                    email
                );
                return false;
            },
        };

        tracing::info!("Account {} refreshing quota in realtime...", email);
        match quota::fetch_quota(&access_token, email).await {
            Ok((quota_data, _project_id)) => {
                let earliest_reset = quota_data
                    .models
                    .iter()
                    .filter_map(|m| {
                        if !m.reset_time.is_empty() {
                            Some(m.reset_time.as_str())
                        } else {
                            None
                        }
                    })
                    .min();

                if let Some(reset_time_str) = earliest_reset {
                    tracing::info!(
                        "Account {} quota refresh successful, reset_time: {}",
                        email,
                        reset_time_str
                    );
                    self.rate_limit_tracker.set_lockout_until_iso(
                        email,
                        reset_time_str,
                        reason,
                        model,
                    )
                } else {
                    tracing::warn!(
                        "Account {} quota refresh successful but no reset_time found",
                        email
                    );
                    false
                }
            },
            Err(e) => {
                tracing::warn!("Account {} quota refresh failed: {:?}", email, e);
                false
            },
        }
    }

    pub async fn mark_rate_limited_async(
        &self,
        account_id: &str,
        status: u16,
        retry_after_header: Option<&str>,
        error_body: &str,
        model: Option<&str>,
    ) {
        let reason = self.rate_limit_tracker.parse_rate_limit_reason(error_body);
        let raw_model = model.unwrap_or("unknown");

        let model_str = crate::proxy::common::model_mapping::normalize_to_standard_id(raw_model)
            .unwrap_or_else(|| raw_model.to_string());

        if reason == RateLimitReason::ModelCapacityExhausted {
            tracing::debug!(
                "{}:{} ModelCapacityExhausted - NOT locking, handler will retry",
                account_id,
                model_str
            );
            return;
        }

        let immediate_lockout = std::time::Duration::from_secs(15);
        self.rate_limit_tracker.set_model_lockout(
            account_id,
            &model_str,
            std::time::SystemTime::now() + immediate_lockout,
            reason,
        );
        tracing::debug!(
            "{}:{} immediate 15s lockout (pending precise time)",
            account_id,
            model_str
        );

        let has_explicit_retry_time =
            retry_after_header.is_some() || error_body.contains("quotaResetDelay");

        if has_explicit_retry_time {
            if let Some(info) = self.rate_limit_tracker.parse_from_error(
                account_id,
                status,
                retry_after_header,
                error_body,
                Some(model_str.clone()),
            ) {
                self.rate_limit_tracker.set_model_lockout(
                    account_id,
                    &model_str,
                    info.reset_time,
                    reason,
                );
            }
            return;
        }

        match reason {
            RateLimitReason::QuotaExhausted => {
                let lockout = std::time::Duration::from_secs(600);
                self.rate_limit_tracker.set_model_lockout(
                    account_id,
                    &model_str,
                    std::time::SystemTime::now() + lockout,
                    reason,
                );
                tracing::info!("{}:{} QUOTA_EXHAUSTED, 10min fallback lock", account_id, model_str);
            },
            RateLimitReason::RateLimitExceeded
            | RateLimitReason::ModelCapacityExhausted
            | RateLimitReason::ServerError
            | RateLimitReason::Unknown => {
                let lockout_secs =
                    self.rate_limit_tracker.set_adaptive_model_lockout(account_id, &model_str);
                tracing::debug!("{}:{} adaptive lockout: {}s", account_id, model_str, lockout_secs);
            },
        }

        if self
            .fetch_and_lock_with_realtime_quota(account_id, reason, Some(model_str.clone()))
            .await
        {
            tracing::info!("{}:{} locked with precise reset time", account_id, model_str);
            return;
        }

        if self.set_precise_lockout(account_id, reason, model.map(|s| s.to_string())).await {
            tracing::info!("{}:{} locked with cached reset time", account_id, model_str);
            return;
        }

        tracing::warn!(
            "{}:{} no precise reset time available, using temporary lock",
            account_id,
            model_str
        );
    }
}
