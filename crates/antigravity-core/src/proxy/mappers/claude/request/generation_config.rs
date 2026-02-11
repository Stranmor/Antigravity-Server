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
use crate::proxy::common::thinking_constants::{THINKING_BUDGET, THINKING_OVERHEAD};
use serde_json::{json, Value};

pub fn build_generation_config(
    claude_req: &ClaudeRequest,
    has_web_search: bool,
    is_thinking_enabled: bool,
) -> Value {
    let mut config = json!({});

    // Thinking config
    if is_thinking_enabled {
        if let Some(thinking) = &claude_req.thinking {
            if thinking.type_ == "enabled" {
                let mut thinking_config = json!({"includeThoughts": true});

                if let Some(budget_tokens) = thinking.budget_tokens {
                    let mut budget = budget_tokens;
                    let is_flash_model =
                        has_web_search || claude_req.model.to_lowercase().contains("flash");
                    if is_flash_model {
                        budget = budget.min(24576);
                    }
                    thinking_config["thinkingBudget"] = json!(budget);
                }

                config["thinkingConfig"] = thinking_config;
            }
        } else {
            // [FIX 2026-02-07] Auto-thinking models (e.g. opus-4-5, opus-4-6) may not have
            // explicit thinking config from client. We MUST inject thinkingConfig
            // with thinkingBudget to ensure upstream actually enables thinking mode.
            // [FIX 2026-02-08] thinkingBudget is REQUIRED by cloudcode-pa API when
            // includeThoughts=true. Without it, API returns 400 INVALID_ARGUMENT.
            let default_budget = THINKING_BUDGET;
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

    // Other parameters
    if let Some(temp) = claude_req.temperature {
        config["temperature"] = json!(temp);
    }
    if let Some(top_p) = claude_req.top_p {
        config["topP"] = json!(top_p);
    }
    if let Some(top_k) = claude_req.top_k {
        config["topK"] = json!(top_k);
    }

    // Effort level mapping (Claude API v2.0.67+)
    // Maps Claude's output_config.effort to Gemini's effortLevel
    if let Some(output_config) = &claude_req.output_config {
        if let Some(effort) = &output_config.effort {
            config["effortLevel"] = json!(match effort.to_lowercase().as_str() {
                "high" => "HIGH",
                "medium" => "MEDIUM",
                "low" => "LOW",
                _ => "HIGH", // Default to HIGH for unknown values
            });
            tracing::debug!(
                "[Generation-Config] Effort level set: {} -> {}",
                effort,
                config["effortLevel"]
            );
        }
    }

    // max_tokens mapping to maxOutputTokens
    // [FIX 2026-02-08] Only set maxOutputTokens when client provides max_tokens explicitly
    // or when thinking is enabled and we need to ensure it exceeds budget.
    // Setting unconditional defaults can cause 400 errors on some model/account combos.
    let mut final_max_tokens: Option<i64> = claude_req.max_tokens.map(|t| t as i64);

    // [NEW] Ensure maxOutputTokens is greater than thinkingBudget (API strict constraint)
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

    // [optimize] Set global stop sequences to prevent streaming output redundancy (control within 4 tokens)
    config["stopSequences"] = json!(["<|user|>", "<|end_of_turn|>", "\n\nHuman:"]);

    config
}
