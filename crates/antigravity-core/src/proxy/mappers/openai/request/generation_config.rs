//! OpenAI generation config building.
#![allow(
    clippy::cast_lossless,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "token budget calculations and JSON field access"
)]

use super::super::models::OpenAIRequest;
use crate::proxy::common::thinking_config::get_thinking_budget_config;
use crate::proxy::common::thinking_constants::{
    THINKING_BUDGET, THINKING_MIN_OVERHEAD, THINKING_OVERHEAD,
};
use antigravity_types::models::ThinkingBudgetMode;
use serde_json::{json, Value};

pub fn build_generation_config(
    request: &OpenAIRequest,
    actual_include_thinking: bool,
    mapped_model: &str,
) -> Value {
    // [FIX 2026-02-08] Sanitize topP: Gemini API rejects 0.0 as invalid argument.
    // Replace 0.0 with the Gemini default (0.95).
    let top_p = match request.top_p {
        Some(v) if v <= 0.0 => {
            tracing::info!("[OpenAI-GenConfig] Sanitizing topP={} to 0.95 (Gemini rejects 0.0)", v);
            0.95
        },
        Some(v) => v as f64,
        None => 0.95,
    };

    let mut gen_config = json!({
        "temperature": request.temperature.unwrap_or(1.0),
        "topP": top_p,
    });

    if let Some(n) = request.n {
        gen_config["candidateCount"] = json!(n);
    }

    if actual_include_thinking {
        let tb_config = get_thinking_budget_config();

        if matches!(tb_config.mode, ThinkingBudgetMode::Adaptive) {
            // Adaptive: use default budget (cloudcode-pa API rejects thinkingBudget:-1)
            let budget = THINKING_BUDGET;
            gen_config["thinkingConfig"] = json!({
                "includeThoughts": true,
                "thinkingBudget": budget
            });
            gen_config["maxOutputTokens"] = json!(budget + THINKING_OVERHEAD);
            tracing::debug!(
                "[OpenAI-Request] Adaptive thinkingConfig for model {}: thinkingBudget={}, maxOutputTokens={}",
                mapped_model,
                budget,
                budget + THINKING_OVERHEAD,
            );
        } else {
            let budget = match tb_config.mode {
                ThinkingBudgetMode::Custom => u64::from(tb_config.custom_value),
                _ => THINKING_BUDGET, // Auto and Passthrough use default
            };

            gen_config["thinkingConfig"] = json!({
                "includeThoughts": true,
                "thinkingBudget": budget
            });

            // maxOutputTokens calculation (same as original logic but with resolved budget)
            if let Some(max_tokens) = request.max_tokens {
                if u64::from(max_tokens) <= budget {
                    gen_config["maxOutputTokens"] = json!(budget + THINKING_MIN_OVERHEAD);
                } else {
                    gen_config["maxOutputTokens"] = json!(max_tokens);
                }
            } else {
                gen_config["maxOutputTokens"] = json!(budget + THINKING_OVERHEAD);
            }

            let new_max = gen_config["maxOutputTokens"].as_i64().unwrap_or(0);
            tracing::debug!(
                "[OpenAI-Request] Injected thinkingConfig for model {}: thinkingBudget={}, maxOutputTokens={}",
                mapped_model,
                budget,
                new_max
            );
        }
    } else {
        // Non-thinking models: only set maxOutputTokens if client explicitly provided it
        if let Some(max_tokens) = request.max_tokens {
            gen_config["maxOutputTokens"] = json!(max_tokens);
        }
    }

    if let Some(stop) = &request.stop {
        if stop.is_string() {
            gen_config["stopSequences"] = json!([stop]);
        } else if stop.is_array() {
            gen_config["stopSequences"] = stop.clone();
        }
    }

    if let Some(fmt) = &request.response_format {
        if fmt.r#type == "json_object" {
            gen_config["responseMimeType"] = json!("application/json");
        }
    }

    gen_config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy::common::thinking_config::{
        update_thinking_budget_config, THINKING_CONFIG_TEST_LOCK,
    };
    use antigravity_types::models::{ThinkingBudgetConfig, ThinkingBudgetMode};

    fn make_openai_req(max_tokens: Option<u32>) -> OpenAIRequest {
        OpenAIRequest {
            model: "gemini-3-pro".to_string(),
            messages: vec![],
            prompt: None,
            stream: false,
            n: None,
            max_tokens,
            temperature: None,
            top_p: None,
            stop: None,
            response_format: None,
            tools: None,
            tool_choice: None,
            parallel_tool_calls: None,
            instructions: None,
            input: None,
            size: None,
            quality: None,
            person_generation: None,
        }
    }

    #[test]
    fn test_openai_auto_mode_default() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Auto,
            ..Default::default()
        });
        let req = make_openai_req(None);
        let config = build_generation_config(&req, true, "gemini-3-pro");
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 16000);
        assert_eq!(config["maxOutputTokens"], 48768); // 16000 + 32768
    }

    #[test]
    fn test_openai_custom_mode() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Custom,
            custom_value: 20000,
            effort: None,
        });
        let req = make_openai_req(None);
        let config = build_generation_config(&req, true, "gemini-3-pro");
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 20000);
        assert_eq!(config["maxOutputTokens"], 52768); // 20000 + 32768
    }

    #[test]
    fn test_openai_adaptive_mode() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Adaptive,
            ..Default::default()
        });
        let req = make_openai_req(None);
        let config = build_generation_config(&req, true, "gemini-3-pro");
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 16000);
        assert_eq!(config["maxOutputTokens"], 48768); // 16000 + 32768
    }

    #[test]
    fn test_openai_auto_small_max_tokens() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Auto,
            ..Default::default()
        });
        let req = make_openai_req(Some(5000));
        let config = build_generation_config(&req, true, "gemini-3-pro");
        assert_eq!(config["thinkingConfig"]["thinkingBudget"], 16000);
        assert_eq!(config["maxOutputTokens"], 24192); // 16000 + 8192
    }
}
