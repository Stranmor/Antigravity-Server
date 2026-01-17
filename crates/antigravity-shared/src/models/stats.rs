use crate::models::account::Account;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
    pub base_url: String,
    pub active_accounts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DashboardStats {
    pub total_accounts: usize,
    pub avg_gemini_quota: i32,
    pub avg_gemini_image_quota: i32,
    pub avg_claude_quota: i32,
    pub low_quota_count: usize,
    pub pro_count: usize,
    pub ultra_count: usize,
    pub free_count: usize,
}

impl DashboardStats {
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
                    // percentage is "remaining percentage" (0-100)
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RefreshStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProxyStats {
    pub total_requests: u64,
    #[serde(alias = "success_requests")]
    pub success_count: u64,
    #[serde(alias = "failed_requests")]
    pub error_count: u64,
    #[serde(default)]
    pub total_input_tokens: u64,
    #[serde(default)]
    pub total_output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyRequestLog {
    pub id: String,
    pub timestamp: i64,
    pub method: String,
    #[serde(alias = "path")]
    pub url: String,
    pub status: u16,
    #[serde(alias = "duration_ms")]
    pub duration: u64,
    pub model: Option<String>,
    pub mapped_model: Option<String>,
    #[serde(alias = "mapping_reason")]
    pub mapping_reason: Option<String>,
    pub account_email: Option<String>,
    #[serde(alias = "error_message")]
    pub error: Option<String>,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_url: Option<String>,
    pub release_notes: Option<String>,
}
