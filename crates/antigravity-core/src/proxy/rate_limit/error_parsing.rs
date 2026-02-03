use std::time::{Duration, SystemTime};

use super::parser;
use super::rate_limit_info::{RateLimitInfo, RateLimitKey, RateLimitReason};
use super::tracker::RateLimitTracker;
use super::{
    FAILURE_COUNT_EXPIRY_SECONDS, QUOTA_LOCKOUT_TIER_1, QUOTA_LOCKOUT_TIER_2, QUOTA_LOCKOUT_TIER_3,
    QUOTA_LOCKOUT_TIER_4, RATE_LIMIT_DEFAULT_SECONDS,
};

impl RateLimitTracker {
    /// Parse rate limit info from error response
    pub fn parse_from_error(
        &self,
        account_id: &str,
        status: u16,
        retry_after_header: Option<&str>,
        body: &str,
        model: Option<String>,
    ) -> Option<RateLimitInfo> {
        if status != 429 && status != 500 && status != 503 && status != 529 {
            return None;
        }

        let reason = if status == 429 {
            tracing::warn!("Google 429 Error Body: {}", body);
            self.parse_rate_limit_reason(body)
        } else {
            RateLimitReason::ServerError
        };

        // ModelCapacityExhausted: don't block account, handler will retry
        if reason == RateLimitReason::ModelCapacityExhausted {
            tracing::debug!(
                "MODEL_CAPACITY_EXHAUSTED для {}: НЕ блокируем, handler сделает retry",
                account_id
            );
            return None;
        }

        let mut retry_after_sec = None;

        if let Some(retry_after) = retry_after_header {
            if let Ok(seconds) = retry_after.parse::<u64>() {
                retry_after_sec = Some(seconds);
            }
        }

        if retry_after_sec.is_none() {
            retry_after_sec = parser::parse_retry_time_from_body(body);
        }

        let retry_sec = match retry_after_sec {
            Some(s) => {
                if s < 2 {
                    2
                } else {
                    s
                }
            }
            None => {
                let failure_count = {
                    let now = SystemTime::now();
                    let key = RateLimitKey::from_optional_model(account_id, model.as_deref());
                    let mut entry = self.failure_counts.entry(key).or_insert((0, now));
                    let elapsed = now
                        .duration_since(entry.1)
                        .unwrap_or(Duration::from_secs(0))
                        .as_secs();
                    if elapsed > FAILURE_COUNT_EXPIRY_SECONDS {
                        tracing::debug!(
                            "account {} failure count expired ({} seconds), reset to 0",
                            account_id,
                            elapsed
                        );
                        *entry = (0, now);
                    }
                    entry.0 += 1;
                    entry.1 = now;
                    entry.0
                };

                match reason {
                    RateLimitReason::QuotaExhausted => {
                        let lockout = match failure_count {
                            1 => {
                                tracing::warn!(
                                    "Detected quota exhausted (QUOTA_EXHAUSTED), 1st failure, locking for 60 seconds"
                                );
                                QUOTA_LOCKOUT_TIER_1
                            }
                            2 => {
                                tracing::warn!(
                                    "Detected quota exhausted (QUOTA_EXHAUSTED), 2nd consecutive failure, locking for 5 minutes"
                                );
                                QUOTA_LOCKOUT_TIER_2
                            }
                            3 => {
                                tracing::warn!(
                                    "Detected quota exhausted (QUOTA_EXHAUSTED), 3rd consecutive failure, locking for 30 minutes"
                                );
                                QUOTA_LOCKOUT_TIER_3
                            }
                            _ => {
                                tracing::warn!(
                                    "Detected quota exhausted (QUOTA_EXHAUSTED), {} consecutive failures, locking for 2 hours",
                                    failure_count
                                );
                                QUOTA_LOCKOUT_TIER_4
                            }
                        };
                        lockout
                    }
                    RateLimitReason::RateLimitExceeded => {
                        tracing::debug!(
                            "Detected rate limit (RATE_LIMIT_EXCEEDED), using default 5 seconds"
                        );
                        RATE_LIMIT_DEFAULT_SECONDS
                    }
                    RateLimitReason::ModelCapacityExhausted => {
                        unreachable!("ModelCapacityExhausted should be handled by early return")
                    }
                    RateLimitReason::ServerError => {
                        tracing::warn!(
                            "Detected 5xx error ({}), applying 20s soft backoff...",
                            status
                        );
                        20
                    }
                    RateLimitReason::Unknown => {
                        tracing::debug!(
                            "Cannot parse 429 rate limit reason, using default 60 seconds"
                        );
                        60
                    }
                }
            }
        };

        let info = RateLimitInfo {
            reset_time: SystemTime::now() + Duration::from_secs(retry_sec),
            retry_after_sec: retry_sec,
            detected_at: SystemTime::now(),
            reason,
            model: model.clone(),
        };

        let key = RateLimitKey::from_optional_model(account_id, model.as_deref());
        self.limits.insert(key, info.clone());

        tracing::warn!(
            "account {} [{}] rate limit type: {:?}, reset delay: {} seconds",
            account_id,
            status,
            reason,
            retry_sec
        );

        Some(info)
    }

    /// Parse rate limit reason from response body
    pub fn parse_rate_limit_reason(&self, body: &str) -> RateLimitReason {
        let trimmed = body.trim();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(reason_str) = json
                    .get("error")
                    .and_then(|e| e.get("details"))
                    .and_then(|d| d.as_array())
                    .and_then(|a| a.first())
                    .and_then(|o| o.get("reason"))
                    .and_then(|v| v.as_str())
                {
                    return match reason_str {
                        "QUOTA_EXHAUSTED" => RateLimitReason::QuotaExhausted,
                        "RATE_LIMIT_EXCEEDED" => RateLimitReason::RateLimitExceeded,
                        "MODEL_CAPACITY_EXHAUSTED" => RateLimitReason::ModelCapacityExhausted,
                        _ => RateLimitReason::Unknown,
                    };
                }
                if let Some(msg) = json
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|v| v.as_str())
                {
                    let msg_lower = msg.to_lowercase();
                    if msg_lower.contains("per minute") || msg_lower.contains("rate limit") {
                        return RateLimitReason::RateLimitExceeded;
                    }
                }
            }
        }

        let body_lower = body.to_lowercase();
        if body_lower.contains("per minute")
            || body_lower.contains("rate limit")
            || body_lower.contains("too many requests")
        {
            RateLimitReason::RateLimitExceeded
        } else if body_lower.contains("exhausted") || body_lower.contains("quota") {
            RateLimitReason::QuotaExhausted
        } else {
            RateLimitReason::Unknown
        }
    }
}
