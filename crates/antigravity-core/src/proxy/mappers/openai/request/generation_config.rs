//! OpenAI generation config building.
#![allow(
    clippy::cast_lossless,
    clippy::as_conversions,
    clippy::indexing_slicing,
    reason = "token budget calculations and JSON field access"
)]

use super::super::models::OpenAIRequest;
use crate::proxy::common::thinking_constants::{
    THINKING_BUDGET, THINKING_MIN_OVERHEAD, THINKING_OVERHEAD,
};
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
        gen_config["thinkingConfig"] = json!({
            "includeThoughts": true,
            "thinkingBudget": THINKING_BUDGET
        });

        // [FIX 2026-02-08] maxOutputTokens MUST be > thinkingBudget (API strict constraint).
        // If client provides max_tokens and it's too small, bump it.
        // If client doesn't provide max_tokens, use budget + overhead (matching upstream).
        if let Some(max_tokens) = request.max_tokens {
            if (max_tokens as i64) <= THINKING_BUDGET as i64 {
                gen_config["maxOutputTokens"] = json!(THINKING_BUDGET + THINKING_MIN_OVERHEAD);
            } else {
                gen_config["maxOutputTokens"] = json!(max_tokens);
            }
        } else {
            // No client max_tokens → use budget + overhead (upstream default)
            gen_config["maxOutputTokens"] = json!(THINKING_BUDGET + THINKING_OVERHEAD);
        }

        let new_max = gen_config["maxOutputTokens"].as_i64().unwrap_or(0);
        tracing::debug!(
            "[OpenAI-Request] Injected thinkingConfig for model {}: thinkingBudget={}, maxOutputTokens={}",
            mapped_model,
            THINKING_BUDGET,
            new_max
        );
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
