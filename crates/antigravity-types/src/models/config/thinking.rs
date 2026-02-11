//! Thinking budget configuration for adaptive AI model thinking modes.

use serde::{Deserialize, Serialize};

/// Thinking budget mode determines how the thinking token budget is resolved.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingBudgetMode {
    /// Automatic: use client's budget, cap Gemini Flash at 24576.
    Auto,
    /// Passthrough: forward client's value unchanged.
    Passthrough,
    /// Custom: use a fixed custom value.
    Custom,
    /// Adaptive: dynamic budget (-1 sentinel) with thinkingLevel for Gemini 3.
    #[default]
    Adaptive,
}

/// Configuration for thinking budget behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkingBudgetConfig {
    /// Budget resolution mode.
    #[serde(default)]
    pub mode: ThinkingBudgetMode,
    /// Fixed budget value for Custom mode.
    #[serde(default = "default_thinking_budget_custom_value")]
    pub custom_value: u32,
    /// Effort level for Adaptive mode (low/medium/high). None maps to HIGH.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

fn default_thinking_budget_custom_value() -> u32 {
    24576
}

impl Default for ThinkingBudgetConfig {
    fn default() -> Self {
        Self { mode: ThinkingBudgetMode::Adaptive, custom_value: 24576, effort: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_adaptive() {
        assert_eq!(ThinkingBudgetConfig::default().mode, ThinkingBudgetMode::Adaptive);
    }

    #[test]
    fn test_default_custom_value() {
        assert_eq!(ThinkingBudgetConfig::default().custom_value, 24576);
    }

    #[test]
    fn test_serde_round_trip_all_modes() {
        for mode in [
            ThinkingBudgetMode::Auto,
            ThinkingBudgetMode::Passthrough,
            ThinkingBudgetMode::Custom,
            ThinkingBudgetMode::Adaptive,
        ] {
            let config = ThinkingBudgetConfig { mode, ..Default::default() };
            let json = serde_json::to_string(&config).expect("serialize");
            let back: ThinkingBudgetConfig = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(config, back, "Round-trip failed for mode {:?}", mode);
        }
    }

    #[test]
    fn test_serde_explicit_auto_mode() {
        let json = r#"{"mode":"auto"}"#;
        let config: ThinkingBudgetConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config.mode, ThinkingBudgetMode::Auto);
        assert_eq!(config.custom_value, 24576); // default filled
    }

    #[test]
    fn test_effort_serialization() {
        let with_effort =
            ThinkingBudgetConfig { effort: Some("high".to_string()), ..Default::default() };
        let json = serde_json::to_string(&with_effort).expect("serialize");
        assert!(json.contains("\"effort\":\"high\""));

        let without_effort = ThinkingBudgetConfig::default();
        let json = serde_json::to_string(&without_effort).expect("serialize");
        assert!(!json.contains("effort")); // skip_serializing_if
    }

    #[test]
    fn test_serde_missing_thinking_budget_produces_default() {
        // Simulate deserializing a ProxyConfig-like struct where thinking_budget is missing
        let json = r#"{}"#;
        let config: ThinkingBudgetConfig = serde_json::from_str(json).expect("deserialize");
        assert_eq!(config, ThinkingBudgetConfig::default());
    }
}
