use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RateLimitReason {
    QuotaExhausted,
    RateLimitExceeded,
    ModelCapacityExhausted,
    ServerError,
    Unknown,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum RateLimitKey {
    Account(String),
    Model { account: String, model: String },
}

impl RateLimitKey {
    pub fn account(account_id: &str) -> Self {
        RateLimitKey::Account(account_id.to_string())
    }

    pub fn model(account_id: &str, model: &str) -> Self {
        RateLimitKey::Model {
            account: account_id.to_string(),
            model: model.to_string(),
        }
    }

    pub fn from_optional_model(account_id: &str, model: Option<&str>) -> Self {
        match model {
            Some(m) => RateLimitKey::model(account_id, m),
            None => RateLimitKey::account(account_id),
        }
    }

    pub fn account_id(&self) -> &str {
        match self {
            RateLimitKey::Account(acc) => acc,
            RateLimitKey::Model { account, .. } => account,
        }
    }

    pub fn model_name(&self) -> Option<&str> {
        match self {
            RateLimitKey::Account(_) => None,
            RateLimitKey::Model { model, .. } => Some(model),
        }
    }
}

impl std::fmt::Display for RateLimitKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitKey::Account(acc) => write!(f, "{}", acc),
            RateLimitKey::Model { account, model } => write!(f, "{}:{}", account, model),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub reset_time: SystemTime,
    #[allow(dead_code)]
    pub retry_after_sec: u64,
    #[allow(dead_code)]
    pub detected_at: SystemTime,
    #[allow(dead_code)]
    pub reason: RateLimitReason,
    #[allow(dead_code)]
    pub model: Option<String>,
}
