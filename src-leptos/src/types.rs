use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// --- Enums ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyAuthMode {
    #[default]
    Off,
    Strict,
    AllExceptHealth,
    Auto,
}

impl fmt::Display for ProxyAuthMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProxyAuthMode::Off => write!(f, "off"),
            ProxyAuthMode::Strict => write!(f, "strict"),
            ProxyAuthMode::AllExceptHealth => write!(f, "all_except_health"),
            ProxyAuthMode::Auto => write!(f, "auto"),
        }
    }
}

impl ProxyAuthMode {
    pub fn from_string(s: &str) -> Self {
        match s {
            "strict" => ProxyAuthMode::Strict,
            "all_except_health" => ProxyAuthMode::AllExceptHealth,
            "auto" => ProxyAuthMode::Auto,
            _ => ProxyAuthMode::Off,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ZaiDispatchMode {
    #[default]
    Off,
    Exclusive,
    Pooled,
    Fallback,
}

impl fmt::Display for ZaiDispatchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZaiDispatchMode::Off => write!(f, "off"),
            ZaiDispatchMode::Exclusive => write!(f, "exclusive"),
            ZaiDispatchMode::Pooled => write!(f, "pooled"),
            ZaiDispatchMode::Fallback => write!(f, "fallback"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Protocol {
    #[default]
    OpenAI,
    Anthropic,
    Gemini,
}

// --- Z.ai Structs ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZaiModelDefaults {
    #[serde(default = "default_zai_opus_model")]
    pub opus: String,
    #[serde(default = "default_zai_sonnet_model")]
    pub sonnet: String,
    #[serde(default = "default_zai_haiku_model")]
    pub haiku: String,
}

impl Default for ZaiModelDefaults {
    fn default() -> Self {
        Self {
            opus: default_zai_opus_model(),
            sonnet: default_zai_sonnet_model(),
            haiku: default_zai_haiku_model(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ZaiMcpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub web_search_enabled: bool,
    #[serde(default)]
    pub web_reader_enabled: bool,
    #[serde(default)]
    pub vision_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ZaiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_zai_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub dispatch_mode: ZaiDispatchMode,
    #[serde(default)]
    pub model_mapping: HashMap<String, String>,
    #[serde(default)]
    pub models: ZaiModelDefaults,
    #[serde(default)]
    pub mcp: ZaiMcpConfig,
}

// --- Other Config Structs ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ExperimentalConfig {
    #[serde(default = "default_true")]
    pub enable_signature_cache: bool,
    #[serde(default = "default_true")]
    pub enable_tool_loop_recovery: bool,
    #[serde(default = "default_true")]
    pub enable_cross_model_checks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StickySessionConfig {
    pub enabled: bool,
    pub mode: String,
    pub ttl: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UpstreamProxyConfig {
    pub enabled: bool,
    pub url: String,
}

// --- Proxy Config ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProxyConfig {
    pub enabled: bool,
    #[serde(default)]
    pub allow_lan_access: bool,
    #[serde(default)]
    pub auth_mode: ProxyAuthMode,
    pub port: u16,
    pub api_key: String,
    pub auto_start: bool,
    #[serde(default)]
    pub custom_mapping: HashMap<String, String>,
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
    #[serde(default)]
    pub enable_logging: bool,
    #[serde(default)]
    pub upstream_proxy: UpstreamProxyConfig,
    #[serde(default)]
    pub zai: ZaiConfig,
    #[serde(default)]
    pub scheduling: StickySessionConfig,
    #[serde(default)]
    pub experimental: ExperimentalConfig,
}

// --- App Config ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
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
    #[serde(default)]
    pub proxy: ProxyConfig,
}

// --- Account Types ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelQuota {
    pub model: String,
    pub used: i32,
    pub limit: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AccountQuota {
    pub models: Vec<ModelQuota>,
    pub is_forbidden: bool,
    pub updated_at: i64,
    pub subscription_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountTokens {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

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

// --- Status & Stats ---

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

                let has_pro = quota
                    .models
                    .iter()
                    .any(|m| m.model.contains("pro") && m.limit > 50);
                let has_ultra = quota.models.iter().any(|m| m.limit > 500);

                if has_ultra {
                    stats.ultra_count += 1;
                } else if has_pro {
                    stats.pro_count += 1;
                } else {
                    stats.free_count += 1;
                }

                let any_low = quota
                    .models
                    .iter()
                    .any(|m| m.limit > 0 && ((m.limit - m.used) * 100 / m.limit) < 20);
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RefreshStats {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProxyStats {
    pub total_requests: u64,
    pub success_requests: u64,
    pub failed_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_url: Option<String>,
    pub release_notes: Option<String>,
}

// --- Helper Functions ---

fn default_true() -> bool {
    true
}

fn default_zai_base_url() -> String {
    "https://api.z.ai/api/anthropic".to_string()
}

fn default_zai_opus_model() -> String {
    "glm-4.7".to_string()
}

fn default_zai_sonnet_model() -> String {
    "glm-4.7".to_string()
}

fn default_zai_haiku_model() -> String {
    "glm-4.5-air".to_string()
}

fn default_request_timeout() -> u64 {
    120
}
