//! Type definitions matching the Tauri backend types.
//!
//! These must stay in sync with src-tauri/src/types/

use serde::{Deserialize, Serialize};

/// Account quota information
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct AccountQuota {
    pub models: Vec<ModelQuota>,
    pub is_forbidden: bool,
    pub updated_at: i64,
    pub subscription_tier: Option<String>,
}

/// Model quota entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelQuota {
    pub model: String,
    pub used: i32,
    pub limit: i32,
}

/// Account data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub disabled: bool,
    pub disabled_reason: Option<String>,
    pub proxy_disabled: bool,
    pub last_used: i64,
    pub quota: Option<AccountQuota>,
    pub tokens: Option<AccountTokens>,
}

/// Account tokens
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountTokens {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

/// Proxy status
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
    pub base_url: String,
    pub active_accounts: usize,
}

/// Proxy configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub port: u16,
    pub api_key: String,
    pub auto_start: bool,
    pub allow_lan_access: Option<bool>,
    pub auth_mode: Option<String>,
    pub request_timeout: i32,
    pub enable_logging: bool,
    pub upstream_proxy: UpstreamProxyConfig,
}

/// Upstream proxy config
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UpstreamProxyConfig {
    pub enabled: bool,
    pub url: String,
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    pub language: String,
    pub theme: String,
    pub auto_refresh: bool,
    pub refresh_interval: i32,
    pub auto_sync: bool,
    pub sync_interval: i32,
    pub default_export_path: Option<String>,
    pub antigravity_executable: Option<String>,
    pub antigravity_args: Option<Vec<String>>,
    pub auto_launch: bool,
    pub proxy: ProxyConfig,
}

/// Dashboard statistics (computed from accounts)
#[derive(Debug, Clone, Default, PartialEq)]
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
    /// Compute stats from accounts list
    pub fn from_accounts(accounts: &[Account]) -> Self {
        let mut stats = Self::default();
        stats.total_accounts = accounts.len();
        
        if accounts.is_empty() {
            return stats;
        }
        
        let mut gemini_sum = 0i32;
        let mut gemini_image_sum = 0i32;
        let mut claude_sum = 0i32;
        
        for account in accounts {
            if let Some(quota) = &account.quota {
                for model in &quota.models {
                    let percent = if model.limit > 0 {
                        ((model.limit - model.used) * 100) / model.limit
                    } else {
                        0
                    };
                    
                    if model.model.contains("gemini-3-pro") || model.model.contains("flash") {
                        gemini_sum += percent;
                    }
                    if model.model.contains("image") {
                        gemini_image_sum += percent;
                    }
                    if model.model.contains("claude") {
                        claude_sum += percent;
                    }
                }
                
                // Detect tier from quota patterns
                let has_pro = quota.models.iter().any(|m| m.model.contains("pro") && m.limit > 50);
                let has_ultra = quota.models.iter().any(|m| m.limit > 500);
                
                if has_ultra {
                    stats.ultra_count += 1;
                } else if has_pro {
                    stats.pro_count += 1;
                } else {
                    stats.free_count += 1;
                }
                
                // Low quota detection
                let any_low = quota.models.iter().any(|m| {
                    m.limit > 0 && ((m.limit - m.used) * 100 / m.limit) < 20
                });
                if any_low {
                    stats.low_quota_count += 1;
                }
            } else {
                stats.free_count += 1;
            }
        }
        
        let n = accounts.len() as i32;
        stats.avg_gemini_quota = gemini_sum / n;
        stats.avg_gemini_image_quota = gemini_image_sum / n;
        stats.avg_claude_quota = claude_sum / n;
        
        stats
    }
}

/// Refresh statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct RefreshStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

/// Proxy statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ProxyStats {
    pub total_requests: u64,
    pub success_requests: u64,
    pub failed_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

/// Proxy request log entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyRequestLog {
    pub id: String,
    pub timestamp: i64,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub duration_ms: u32,
    pub model: Option<String>,
    pub mapped_model: Option<String>,
    pub account_email: Option<String>,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub error_message: Option<String>,
}

/// Update information
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_url: Option<String>,
    pub release_notes: Option<String>,
}
