use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AimdAccountStats {
    pub account_id: String,
    pub confirmed_limit: u64,
    pub ceiling: u64,
    pub requests_this_minute: u64,
    pub working_threshold: u64,
    pub usage_ratio: f64,
}
