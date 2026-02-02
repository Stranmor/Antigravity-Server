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

impl ModelQuota {
    /// Parse reset_time string (e.g., "4h 30m", "0h 0m", "45m") into total seconds.
    /// Returns 0 if parsing fails or time is already expired.
    pub fn reset_time_seconds(&self) -> i64 {
        parse_reset_time(&self.reset_time)
    }

    /// Check if this model's quota has reset (reset_time <= 0).
    pub fn has_reset(&self) -> bool {
        self.reset_time_seconds() <= 0
    }
}

/// Parse a reset time string like "4h 30m", "0h 0m", "45m", "2h" into seconds.
/// Returns 0 for unparseable strings or already-expired times.
pub fn parse_reset_time(s: &str) -> i64 {
    let s = s.trim();
    let mut total_seconds: i64 = 0;

    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'h' || b == b'H' {
            let num_end = i;
            let num_start = bytes[..num_end]
                .iter()
                .rposition(|&c| !c.is_ascii_digit())
                .map(|p| p + 1)
                .unwrap_or(0);
            if let Ok(hours) = std::str::from_utf8(&bytes[num_start..num_end])
                .unwrap_or("")
                .parse::<i64>()
            {
                total_seconds += hours * 3600;
            }
        } else if b == b'm' || b == b'M' {
            let num_end = i;
            let num_start = bytes[..num_end]
                .iter()
                .rposition(|&c| !c.is_ascii_digit())
                .map(|p| p + 1)
                .unwrap_or(0);
            if let Ok(minutes) = std::str::from_utf8(&bytes[num_start..num_end])
                .unwrap_or("")
                .parse::<i64>()
            {
                total_seconds += minutes * 60;
            }
        }
    }

    total_seconds
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

    /// Check if any model's quota has reset and needs refresh.
    /// Returns true if any model has reset_time <= 0 (quota already refreshed by Google).
    pub fn needs_refresh(&self) -> bool {
        self.models.iter().any(|m| m.has_reset())
    }

    /// Get the minimum reset time in seconds across all models.
    pub fn min_reset_seconds(&self) -> Option<i64> {
        self.models.iter().map(|m| m.reset_time_seconds()).min()
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

    #[test]
    fn test_parse_reset_time() {
        assert_eq!(parse_reset_time("4h 30m"), 4 * 3600 + 30 * 60);
        assert_eq!(parse_reset_time("0h 0m"), 0);
        assert_eq!(parse_reset_time("1h"), 3600);
        assert_eq!(parse_reset_time("45m"), 45 * 60);
        assert_eq!(parse_reset_time("0h 5m"), 5 * 60);
        assert_eq!(parse_reset_time(""), 0);
        assert_eq!(parse_reset_time("invalid"), 0);
        assert_eq!(parse_reset_time("30m 4h"), 4 * 3600 + 30 * 60);
    }

    #[test]
    fn test_needs_refresh() {
        let mut quota = QuotaData::new();
        quota.add_model("g3-pro".to_string(), 100, "0h 0m".to_string());
        quota.add_model("g3-flash".to_string(), 100, "4h 30m".to_string());

        assert!(quota.needs_refresh());
        assert_eq!(quota.min_reset_seconds(), Some(0));
    }

    #[test]
    fn test_no_refresh_needed() {
        let mut quota = QuotaData::new();
        quota.add_model("g3-pro".to_string(), 50, "2h 15m".to_string());
        quota.add_model("g3-flash".to_string(), 80, "4h 30m".to_string());

        assert!(!quota.needs_refresh());
        assert_eq!(quota.min_reset_seconds(), Some(2 * 3600 + 15 * 60));
    }
}
