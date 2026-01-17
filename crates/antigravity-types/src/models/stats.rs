//! Statistics and monitoring models.

use super::Account;
use serde::{Deserialize, Serialize};

/// Proxy service status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
    pub fn from_accounts(accounts: &[Account]) -> Self {
        let mut stats = Self {
            total_accounts: accounts.len(),
            ..Default::default()
        };

        if accounts.is_empty() {
            return stats;
        }

        let mut gemini_sum = 0i32;
        let mut gemini_image_sum = 0i32;
        let mut claude_sum = 0i32;

        for account in accounts {
            if let Some(quota) = &account.quota {
                for model in &quota.models {
                    let percent = model.percentage;

                    if model.name.contains("gemini-3-pro") || model.name.contains("flash") {
                        gemini_sum += percent;
                    }
                    if model.name.contains("image") {
                        gemini_image_sum += percent;
                    }
                    if model.name.contains("claude") {
                        claude_sum += percent;
                    }
                }

                // Tier determination
                let tier = quota
                    .subscription_tier
                    .clone()
                    .unwrap_or_default()
                    .to_lowercase();
                if tier.contains("ultra") {
                    stats.ultra_count += 1;
                } else if tier.contains("pro") {
                    stats.pro_count += 1;
                } else {
                    stats.free_count += 1;
                }

                // Low quota check (remaining < 20%)
                let any_low = quota.models.iter().any(|m| m.percentage < 20);
                if any_low {
                    stats.low_quota_count += 1;
                }
            } else {
                stats.free_count += 1;
            }
        }

        let n = accounts.len() as i32;
        if n > 0 {
            stats.avg_gemini_quota = gemini_sum / n;
            stats.avg_gemini_image_quota = gemini_image_sum / n;
            stats.avg_claude_quota = claude_sum / n;
        }

        stats
    }
}

/// Token refresh operation statistics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RefreshStats {
    /// Total accounts attempted
    pub total: usize,
    /// Successfully refreshed
    pub success: usize,
    /// Failed to refresh
    pub failed: usize,
}

/// Proxy request statistics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 100.0;
        }
        (self.success_count as f64 / self.total_requests as f64) * 100.0
    }
}

/// Individual proxy request log entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    pub request_body: Option<String>,
    /// Response body (truncated)
    pub response_body: Option<String>,
    /// Input tokens used
    pub input_tokens: Option<u32>,
    /// Output tokens generated
    pub output_tokens: Option<u32>,
}

/// Application update information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
