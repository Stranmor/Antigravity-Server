//! Statistics and monitoring models.

use super::model_family::ModelFamily;
use super::Account;
use serde::{Deserialize, Serialize};

/// Proxy service status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProxyStatus {
    /// Whether the proxy is running
    pub running: bool,
    /// Port the proxy is listening on
    pub port: u16,
    /// Base URL for the proxy
    pub base_url: String,
    /// Number of active accounts in the pool
    pub active_accounts: usize,
}

/// Dashboard statistics derived from account data.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DashboardStats {
    /// Total number of accounts
    pub total_accounts: usize,
    /// Average Gemini quota percentage
    pub avg_gemini_quota: i32,
    /// Average Gemini image generation quota percentage
    pub avg_gemini_image_quota: i32,
    /// Average Claude quota percentage
    pub avg_claude_quota: i32,
    /// Number of accounts with low quota (< 20%)
    pub low_quota_count: usize,
    /// Number of Pro tier accounts
    pub pro_count: usize,
    /// Number of Ultra tier accounts
    pub ultra_count: usize,
    /// Number of Free tier accounts
    pub free_count: usize,
}

impl DashboardStats {
    /// Calculate statistics from a list of accounts.
    #[allow(
        clippy::arithmetic_side_effects,
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_possible_wrap,
        clippy::cast_precision_loss,
        clippy::float_arithmetic,
        clippy::integer_division,
        reason = "Statistics calculation with bounded values and float averages"
    )]
    pub fn from_accounts(accounts: &[Account]) -> Self {
        let mut stats = Self { total_accounts: accounts.len(), ..Default::default() };

        if accounts.is_empty() {
            return stats;
        }

        let mut gemini_sum = 0f64;
        let mut gemini_image_sum = 0f64;
        let mut claude_sum = 0f64;
        let mut gemini_account_count = 0i32;
        let mut gemini_image_account_count = 0i32;
        let mut claude_account_count = 0i32;

        for account in accounts {
            if let Some(ref quota) = account.quota {
                let mut gemini_total = 0i32;
                let mut gemini_count = 0i32;
                let mut gemini_image_total = 0i32;
                let mut gemini_image_count = 0i32;
                let mut claude_total = 0i32;
                let mut claude_count = 0i32;

                for model in &quota.models {
                    let percent = model.percentage;

                    if ModelFamily::from_model_name(&model.name).is_gemini() {
                        gemini_total += percent;
                        gemini_count += 1;
                    }
                    if ModelFamily::from_model_name(&model.name).is_gemini()
                        && model.name.contains("image")
                    {
                        gemini_image_total += percent;
                        gemini_image_count += 1;
                    }
                    if ModelFamily::from_model_name(&model.name).is_claude() {
                        claude_total += percent;
                        claude_count += 1;
                    }
                }

                if gemini_count > 0_i32 {
                    gemini_sum += f64::from(gemini_total) / f64::from(gemini_count);
                    gemini_account_count += 1;
                }
                if gemini_image_count > 0_i32 {
                    gemini_image_sum +=
                        f64::from(gemini_image_total) / f64::from(gemini_image_count);
                    gemini_image_account_count += 1;
                }
                if claude_count > 0_i32 {
                    claude_sum += f64::from(claude_total) / f64::from(claude_count);
                    claude_account_count += 1;
                }

                // Tier determination
                let tier = quota.subscription_tier.as_deref().unwrap_or_default().to_lowercase();
                if tier.contains("ultra") {
                    stats.ultra_count += 1;
                } else if tier.contains("pro") {
                    stats.pro_count += 1;
                } else {
                    stats.free_count += 1;
                }

                // Low quota check (remaining < 20%)
                let any_low = quota
                    .models
                    .iter()
                    .any(|m| m.percentage < super::quota::QuotaData::LOW_QUOTA_THRESHOLD);
                if any_low {
                    stats.low_quota_count += 1;
                }
            } else {
                stats.free_count += 1;
            }
        }

        #[allow(
            clippy::as_conversions,
            reason = "usize to i32 is safe for account counts < 2 billion"
        )]
        if gemini_account_count > 0_i32 {
            stats.avg_gemini_quota = (gemini_sum / f64::from(gemini_account_count)).round() as i32;
        }
        if gemini_image_account_count > 0_i32 {
            stats.avg_gemini_image_quota =
                (gemini_image_sum / f64::from(gemini_image_account_count)).round() as i32;
        }
        if claude_account_count > 0_i32 {
            stats.avg_claude_quota = (claude_sum / f64::from(claude_account_count)).round() as i32;
        }

        stats
    }
}

/// Token refresh operation statistics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RefreshStats {
    /// Total accounts attempted
    pub total: usize,
    /// Successfully refreshed
    pub success: usize,
    /// Failed to refresh
    pub failed: usize,
}

/// Proxy request statistics.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProxyStats {
    /// Total requests processed
    pub total_requests: u64,
    /// Successful requests
    #[serde(alias = "success_requests")]
    pub success_count: u64,
    /// Failed requests
    #[serde(alias = "failed_requests")]
    pub error_count: u64,
    /// Total input tokens processed
    #[serde(default)]
    pub total_input_tokens: u64,
    /// Total output tokens generated
    #[serde(default)]
    pub total_output_tokens: u64,
}

impl ProxyStats {
    /// Calculate success rate as a percentage.
    #[allow(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        clippy::float_arithmetic,
        reason = "Percentage calculation requires floating point, u64 to f64 is safe for counts"
    )]
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 100.0;
        }
        (self.success_count as f64 / self.total_requests as f64) * 100.0
    }
}

/// Individual proxy request log entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyRequestLog {
    /// Unique request ID
    pub id: String,
    /// Request timestamp
    pub timestamp: i64,
    /// HTTP method
    pub method: String,
    /// Request URL/path
    #[serde(alias = "path")]
    pub url: String,
    /// Response status code
    pub status: u16,
    /// Request duration in milliseconds
    #[serde(alias = "duration_ms")]
    pub duration: u64,
    /// Requested model
    pub model: Option<String>,
    /// Model after mapping
    pub mapped_model: Option<String>,
    /// Reason for model mapping
    #[serde(alias = "mapping_reason")]
    pub mapping_reason: Option<String>,
    /// Account email used
    pub account_email: Option<String>,
    /// Error message if failed
    #[serde(alias = "error_message")]
    pub error: Option<String>,
    /// Request body (truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<String>,
    /// Response body (truncated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_body: Option<String>,
    /// Input tokens used (non-cached)
    pub input_tokens: Option<u32>,
    /// Output tokens generated
    pub output_tokens: Option<u32>,
    /// Cached input tokens (from prompt cache)
    pub cached_tokens: Option<u32>,
}

/// Token usage statistics over a time period.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct TokenUsageStats {
    /// Total input tokens consumed.
    pub total_input: u64,
    /// Total output tokens generated.
    pub total_output: u64,
    /// Total cached tokens used.
    pub total_cached: u64,
    /// Total number of requests.
    pub total_requests: u64,
    /// Time range in seconds.
    pub time_range_secs: u64,
    /// Average input tokens per request.
    pub avg_input_per_request: f64,
    /// Average output tokens per request.
    pub avg_output_per_request: f64,
    /// Average cached tokens per request.
    pub avg_cached_per_request: f64,
    /// Average tokens per minute.
    pub avg_tokens_per_minute: f64,
    /// Average tokens per hour.
    pub avg_tokens_per_hour: f64,
    /// Average tokens per day.
    pub avg_tokens_per_day: f64,
    /// Requests in the last hour.
    pub requests_last_hour: u64,
    /// Tokens used in the last hour.
    pub tokens_last_hour: u64,
    /// Cached tokens in the last hour.
    pub cached_last_hour: u64,
    /// Requests in the last 24 hours.
    pub requests_last_24h: u64,
    /// Tokens used in the last 24 hours.
    pub tokens_last_24h: u64,
    /// Cached tokens in the last 24 hours.
    pub cached_last_24h: u64,
    /// Cache hit rate as percentage.
    pub cache_hit_rate: f64,
}

/// Application update information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UpdateInfo {
    /// Whether an update is available
    pub available: bool,
    /// Current installed version
    pub current_version: String,
    /// Latest available version
    pub latest_version: String,
    /// Release URL
    pub release_url: Option<String>,
    /// Release notes
    pub release_notes: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{Account, DashboardStats};
    use crate::models::{QuotaData, TokenData};

    #[test]
    fn gemini_image_quota_excludes_non_gemini_models() {
        let token =
            TokenData::new("access".to_string(), "refresh".to_string(), 3600, None, None, None);
        let mut account = Account::new("acc-1".to_string(), "user@example.com".to_string(), token);
        let mut quota = QuotaData::new();
        quota.add_model("gemini-image-3".to_string(), 40, "1h".to_string());
        quota.add_model("claude-image-1".to_string(), 80, "1h".to_string());
        account.update_quota(quota);

        let stats = DashboardStats::from_accounts(&[account]);

        assert_eq!(stats.avg_gemini_image_quota, 40);
    }
}
