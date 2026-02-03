//! Generation config building.

use super::super::models::ClaudeRequest;
use serde_json::{json, Value};

pub fn build_generation_config(
    claude_req: &ClaudeRequest,
    has_web_search: bool,
    is_thinking_enabled: bool,
) -> Value {
    let mut config = json!({});

    // Thinking 配置
    if let Some(thinking) = &claude_req.thinking {
        // [New Check] 必须 is_thinking_enabled 为真才生成 thinkingConfig
        if thinking.type_ == "enabled" && is_thinking_enabled {
            let mut thinking_config = json!({"includeThoughts": true});

            if let Some(budget_tokens) = thinking.budget_tokens {
                let mut budget = budget_tokens;
                // [FIX] Broaden check to support all Flash thinking models (e.g. gemini-2.0-flash-thinking)
                let is_flash_model =
                    has_web_search || claude_req.model.to_lowercase().contains("flash");
                if is_flash_model {
                    budget = budget.min(24576);
                }
                thinking_config["thinkingBudget"] = json!(budget);
            }

            config["thinkingConfig"] = thinking_config;
        }
    }

    // 其他参数
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

    // web_search 强制 candidateCount=1
    /*if has_web_search {
        config["candidateCount"] = json!(1);
    }*/

    // max_tokens 映射为 maxOutputTokens
    // [FIX] Use client's max_tokens if provided, otherwise use high default (65536)
    // Gemini supports up to 65536 output tokens for most models
    let mut final_max_tokens: i64 = claude_req.max_tokens.map(|t| t as i64).unwrap_or(65536);

    // [NEW] 确保 maxOutputTokens 大于 thinkingBudget (API 强约束)
    if let Some(thinking_config) = config.get("thinkingConfig") {
        if let Some(budget) = thinking_config
            .get("thinkingBudget")
            .and_then(|t| t.as_u64())
        {
            if final_max_tokens <= budget as i64 {
                final_max_tokens = (budget + 8192) as i64;
                tracing::info!(
                    "[Generation-Config] Bumping maxOutputTokens to {} due to thinking budget of {}",
                    final_max_tokens,
                    budget
                );
            }
        }
    }

    config["maxOutputTokens"] = json!(final_max_tokens);

    // [优化] 设置全局停止序列,防止流式输出冗余 (控制在 4 个以内)
    config["stopSequences"] = json!(["<|user|>", "<|end_of_turn|>", "\n\nHuman:"]);

    config
}
