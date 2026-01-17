//! Quota data models.

use serde::{Deserialize, Serialize};

/// Model quota information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelQuota {
    /// Model name
    pub name: String,
    /// Remaining percentage (0-100)
    pub percentage: i32,
    /// Time when quota resets
    pub reset_time: String,
}

/// Aggregated quota data for an account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct QuotaData {
    /// Per-model quota information
    pub models: Vec<ModelQuota>,
    /// Last time quota was updated
    pub last_updated: i64,
    /// Whether the account is in forbidden state
    #[serde(default)]
    pub is_forbidden: bool,
    /// Subscription tier (FREE/PRO/ULTRA)
    #[serde(default)]
    pub subscription_tier: Option<String>,
}

impl QuotaData {
    /// Create empty quota data.
    pub fn new() -> Self {
        Self {
            models: Vec::new(),
            last_updated: chrono::Utc::now().timestamp(),
            is_forbidden: false,
            subscription_tier: None,
        }
    }

    /// Add a model quota entry.
    pub fn add_model(&mut self, name: String, percentage: i32, reset_time: String) {
        self.models.push(ModelQuota {
            name,
            percentage,
            reset_time,
        });
    }

    /// Get quota for a specific model by name prefix.
    pub fn get_model_quota(&self, prefix: &str) -> Option<&ModelQuota> {
        let prefix_lower = prefix.to_lowercase();
        self.models
            .iter()
            .find(|m| m.name.to_lowercase().contains(&prefix_lower))
    }

    /// Check if any model is below the given threshold percentage.
    pub fn any_below_threshold(&self, threshold: i32) -> bool {
        self.models.iter().any(|m| m.percentage < threshold)
    }

    /// Get the minimum quota percentage across all models.
    pub fn min_quota(&self) -> Option<i32> {
        self.models.iter().map(|m| m.percentage).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_threshold() {
        let mut quota = QuotaData::new();
        quota.add_model("claude-sonnet".to_string(), 50, "5h".to_string());
        quota.add_model("gemini-pro".to_string(), 15, "2h".to_string());

        assert!(quota.any_below_threshold(20));
        assert!(!quota.any_below_threshold(10));
        assert_eq!(quota.min_quota(), Some(15));
    }
}
