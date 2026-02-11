//! Generation config building.

// Generation config uses arithmetic for token budget calculations.
// Values are bounded by model limits (max 24576 for Flash, etc.).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "Generation config: bounded token budgets, JSON object field access"
)]

use super::super::models::ClaudeRequest;
use crate::proxy::common::thinking_config::get_thinking_budget_config;
use crate::proxy::common::thinking_constants::{THINKING_BUDGET, THINKING_OVERHEAD};
use antigravity_types::models::ThinkingBudgetMode;
use serde_json::{json, Value};

pub fn build_generation_config(
    claude_req: &ClaudeRequest,
    has_web_search: bool,
    is_thinking_enabled: bool,
) -> Value {
    let mut config = json!({});
    let tb_config = get_thinking_budget_config();

    if is_thinking_enabled {
        if let Some(thinking) = &claude_req.thinking {
            if thinking.type_ == "enabled" {
                let mut thinking_config = json!({"includeThoughts": true});

                let budget: u64 = if let Some(client_budget) = thinking.budget_tokens {
                    match tb_config.mode {
                        ThinkingBudgetMode::Auto | ThinkingBudgetMode::Adaptive => {
                            let mut b = client_budget;
                            let is_flash_model =
                                has_web_search || claude_req.model.to_lowercase().contains("flash");
                            if is_flash_model {
                                b = b.min(24576);
                            }
                            u64::from(b)
                        },
                        ThinkingBudgetMode::Passthrough => u64::from(client_budget),
                        ThinkingBudgetMode::Custom => u64::from(tb_config.custom_value),
                    }
                } else {
                    match tb_config.mode {
                        ThinkingBudgetMode::Custom => u64::from(tb_config.custom_value),
                        _ => THINKING_BUDGET,
                    }
                };

                if budget > 0 {
                    thinking_config["thinkingBudget"] = json!(budget);
                }

                config["thinkingConfig"] = thinking_config;
            }
        } else if matches!(tb_config.mode, ThinkingBudgetMode::Adaptive) {
            let default_budget = THINKING_BUDGET;
            tracing::info!(
                "[Generation-Config] Auto-injecting thinkingConfig for model: {} (budget: {}, adaptive fallback)",
                claude_req.model,
                default_budget,
            );
            config["thinkingConfig"] = json!({
                "includeThoughts": true,
                "thinkingBudget": default_budget
            });
        } else {
            let default_budget = match tb_config.mode {
                ThinkingBudgetMode::Custom => u64::from(tb_config.custom_value),
                _ => THINKING_BUDGET,
            };
            tracing::info!(
                "[Generation-Config] Auto-injecting thinkingConfig for model: {} (budget: {})",
                claude_req.model,
                default_budget
            );
            config["thinkingConfig"] = json!({
                "includeThoughts": true,
                "thinkingBudget": default_budget
            });
        }
    }

    if let Some(temp) = claude_req.temperature {
        config["temperature"] = json!(temp);
    }
    if let Some(top_p) = claude_req.top_p {
        config["topP"] = json!(top_p);
    }
    if let Some(top_k) = claude_req.top_k {
        config["topK"] = json!(top_k);
    }

    if let Some(output_config) = &claude_req.output_config {
        if let Some(effort) = &output_config.effort {
            config["effortLevel"] = json!(match effort.to_lowercase().as_str() {
                "high" => "HIGH",
                "medium" => "MEDIUM",
                "low" => "LOW",
                _ => "HIGH",
            });
            tracing::debug!(
                "[Generation-Config] Effort level set: {} -> {}",
                effort,
                config["effortLevel"]
            );
        }
    }

    let mut final_max_tokens: Option<i64> = claude_req.max_tokens.map(|t| t as i64);

    if let Some(thinking_config) = config.get("thinkingConfig") {
        if let Some(budget) = thinking_config.get("thinkingBudget").and_then(|t| t.as_u64()) {
            let current = final_max_tokens.unwrap_or(0);
            if current <= budget as i64 {
                final_max_tokens = Some((budget + THINKING_OVERHEAD) as i64);
                tracing::info!(
                    "[Generation-Config] Bumping maxOutputTokens to {} due to thinking budget of {}",
                    final_max_tokens.unwrap_or(0),
                    budget
                );
            }
        }
    }

    if let Some(max_tokens) = final_max_tokens {
        config["maxOutputTokens"] = json!(max_tokens);
    }

    // Merge hardcoded stop sequences with client-provided ones, dedup, cap at 5 (Gemini limit).
    let default_stops = ["<|user|>", "<|end_of_turn|>", "\n\nHuman:"];
    let mut stop_set = std::collections::HashSet::new();
    let mut stop_seqs: Vec<String> = Vec::new();
    for s in default_stops {
        if stop_set.insert(s.to_string()) {
            stop_seqs.push(s.to_string());
        }
    }
    if let Some(client_stops) = &claude_req.stop_sequences {
        for s in client_stops {
            if stop_set.insert(s.clone()) {
                stop_seqs.push(s.clone());
            }
        }
    }
    stop_seqs.truncate(5);
    config["stopSequences"] = json!(stop_seqs);

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::common::thinking_config::{
        update_thinking_budget_config, THINKING_CONFIG_TEST_LOCK,
    };
    use crate::proxy::mappers::claude::claude_models::{ClaudeRequest, ThinkingConfig};
    use antigravity_types::models::ThinkingBudgetConfig;

    fn make_claude_req(thinking: Option<ThinkingConfig>, max_tokens: Option<u32>) -> ClaudeRequest {
        ClaudeRequest {
            model: "claude-opus-4-6".to_string(),
            messages: vec![],
            system: None,
            tools: None,
            stream: false,
            max_tokens,
            temperature: None,
            top_p: None,
            top_k: None,
            thinking,
            stop_sequences: None,
            metadata: None,
            output_config: None,
        }
    }

    #[test]
    fn test_no_client_thinking_injects_default_budget() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Auto,
            ..Default::default()
        });
        let req = make_claude_req(None, None);
        let config = build_generation_config(&req, false, true);
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 16000);
        assert_eq!(config["maxOutputTokens"], 48768);
    }

    #[test]
    fn test_client_budget_passthrough() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Auto,
            ..Default::default()
        });
        let req = make_claude_req(
            Some(ThinkingConfig { type_: "enabled".to_string(), budget_tokens: Some(5000) }),
            None,
        );
        let config = build_generation_config(&req, false, true);
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 5000);
        assert_eq!(config["maxOutputTokens"], 37768);
    }

    #[test]
    fn test_flash_model_caps_budget() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Auto,
            ..Default::default()
        });
        let mut req = make_claude_req(
            Some(ThinkingConfig { type_: "enabled".to_string(), budget_tokens: Some(30000) }),
            None,
        );
        req.model = "claude-3-5-flash".to_string();
        let config = build_generation_config(&req, false, true);
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 24576);
    }

    #[test]
    fn test_web_search_caps_budget_as_flash() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Auto,
            ..Default::default()
        });
        let req = make_claude_req(
            Some(ThinkingConfig { type_: "enabled".to_string(), budget_tokens: Some(30000) }),
            None,
        );
        let config = build_generation_config(&req, true, true);
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 24576);
    }

    #[test]
    fn test_thinking_disabled_no_thinking_config() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Auto,
            ..Default::default()
        });
        let req = make_claude_req(None, None);
        let config = build_generation_config(&req, false, false);
        assert!(config.get("thinkingConfig").is_none());
        assert!(config.get("maxOutputTokens").is_none());
    }

    #[test]
    fn test_stop_sequences_default_only() {
        let req = make_claude_req(None, None);
        let config = build_generation_config(&req, false, false);
        let stops = config["stopSequences"].as_array().unwrap();
        assert_eq!(stops.len(), 3);
    }

    #[test]
    fn test_stop_sequences_client_merged() {
        let mut req = make_claude_req(None, None);
        req.stop_sequences = Some(vec!["STOP".to_string(), "END".to_string()]);
        let config = build_generation_config(&req, false, false);
        let stops = config["stopSequences"].as_array().unwrap();
        assert_eq!(stops.len(), 5);
        assert!(stops.iter().any(|v| v.as_str() == Some("STOP")));
        assert!(stops.iter().any(|v| v.as_str() == Some("END")));
    }

    #[test]
    fn test_stop_sequences_dedup() {
        let mut req = make_claude_req(None, None);
        req.stop_sequences = Some(vec!["\n\nHuman:".to_string()]);
        let config = build_generation_config(&req, false, false);
        let stops = config["stopSequences"].as_array().unwrap();
        assert_eq!(stops.len(), 3);
    }

    #[test]
    fn test_stop_sequences_cap_at_5() {
        let mut req = make_claude_req(None, None);
        req.stop_sequences =
            Some(vec!["A".to_string(), "B".to_string(), "C".to_string(), "D".to_string()]);
        let config = build_generation_config(&req, false, false);
        let stops = config["stopSequences"].as_array().unwrap();
        assert_eq!(stops.len(), 5);
    }
}
