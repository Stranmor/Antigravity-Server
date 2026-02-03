use super::super::models::OpenAIRequest;
use serde_json::{json, Value};

pub fn build_generation_config(
    request: &OpenAIRequest,
    actual_include_thinking: bool,
    mapped_model: &str,
) -> Value {
    const THINKING_BUDGET: u32 = 16000;

    let mut gen_config = json!({
        "temperature": request.temperature.unwrap_or(1.0),
        "topP": request.top_p.unwrap_or(0.95),
    });

    if let Some(max_tokens) = request.max_tokens {
        gen_config["maxOutputTokens"] = json!(max_tokens);
    }

    if let Some(n) = request.n {
        gen_config["candidateCount"] = json!(n);
    }

    if actual_include_thinking {
        gen_config["thinkingConfig"] = json!({
            "includeThoughts": true,
            "thinkingBudget": THINKING_BUDGET
        });

        let current_max = gen_config["maxOutputTokens"].as_i64().unwrap_or(0);
        if current_max <= THINKING_BUDGET as i64 {
            let new_max = THINKING_BUDGET + 8192;
            gen_config["maxOutputTokens"] = json!(new_max);
            tracing::debug!(
                "[OpenAI-Request] Adjusted maxOutputTokens to {} for thinking model (budget={})",
                new_max,
                THINKING_BUDGET
            );
        }

        tracing::debug!(
            "[OpenAI-Request] Injected thinkingConfig for model {}: thinkingBudget={}",
            mapped_model,
            THINKING_BUDGET
        );
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
