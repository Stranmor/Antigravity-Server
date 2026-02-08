//! Tests for OpenAI → Gemini request transformation

use super::*;

#[test]
fn test_transform_openai_request_multimodal() {
    let req = OpenAIRequest {
        model: "gpt-4-vision".to_string(),
        messages: vec![OpenAIMessage {
            role: "user".to_string(),
            content: Some(OpenAIContent::Array(vec![
                OpenAIContentBlock::Text {
                    text: "What is in this image?".to_string(),
                },
                OpenAIContentBlock::ImageUrl {
                    image_url: OpenAIImageUrl {
                        url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==".to_string(),
                        detail: None,
                    },
                },
            ])),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        stream: false,
        n: None,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        response_format: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        instructions: None,
        input: None,
        prompt: None,
        size: None,
        quality: None,
        person_generation: None,
    };

    let result = transform_openai_request(&req, "test-v", "gemini-1.5-flash");
    let parts = &result["request"]["contents"][0]["parts"];
    assert_eq!(parts.as_array().unwrap().len(), 2);
    assert_eq!(parts[0]["text"].as_str().unwrap(), "What is in this image?");
    assert_eq!(parts[1]["inlineData"]["mimeType"].as_str().unwrap(), "image/png");
}

#[test]
fn test_tool_response_compression() {
    let large_content = "x".repeat(250_000);
    let req = OpenAIRequest {
        model: "gpt-4".to_string(),
        messages: vec![
            OpenAIMessage {
                role: "assistant".to_string(),
                content: None,
                reasoning_content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_123".to_string(),
                    r#type: "function".to_string(),
                    function: ToolFunction {
                        name: "read_file".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
                name: None,
            },
            OpenAIMessage {
                role: "tool".to_string(),
                content: Some(OpenAIContent::String(large_content.clone())),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: Some("call_123".to_string()),
                name: Some("read_file".to_string()),
            },
        ],
        stream: false,
        n: None,
        max_tokens: None,
        temperature: None,
        top_p: None,
        stop: None,
        response_format: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        instructions: None,
        input: None,
        prompt: None,
        size: None,
        quality: None,
        person_generation: None,
    };

    let result = transform_openai_request(&req, "test-v", "gemini-1.5-flash");

    let contents = result["request"]["contents"].as_array().unwrap();
    let tool_response_msg = contents
        .iter()
        .find(|c| {
            c["parts"]
                .as_array()
                .map(|p| p.iter().any(|part| part.get("functionResponse").is_some()))
                .unwrap_or(false)
        })
        .expect("Should have tool response message");

    let func_response = &tool_response_msg["parts"][0]["functionResponse"];
    let result_text = func_response["response"]["result"].as_str().unwrap();

    assert!(
        result_text.len() <= 200_100,
        "Tool result should be compressed to approximately 200k chars (got {})",
        result_text.len()
    );
    assert!(
        result_text.len() < large_content.len(),
        "Compressed result should be smaller than original"
    );
}

/// Reproduces exact failing production request (2026-02-08):
/// POST /v1/chat/completions with model=gemini-3-pro-high, response_format=json_object,
/// temperature=1.0, top_p=0.0 → Upstream error (HTTP 400) INVALID_ARGUMENT
///
/// Root causes identified:
/// 1. gemini-3-pro-high maps to gemini-3-pro-preview which requires thinkingConfig
/// 2. maxOutputTokens=65536 must not exceed model limit when thinking is enabled
/// 3. top_p=0.0 must be sanitized (Gemini API rejects 0.0)
/// 4. responseMimeType and thinkingConfig combination must be valid
#[test]
fn test_gemini_3_pro_high_json_mode_with_thinking() {
    let req = OpenAIRequest {
        model: "gemini-3-pro-high".to_string(),
        messages: vec![OpenAIMessage {
            role: "user".to_string(),
            content: Some(OpenAIContent::String(
                "## ТВОЯ ЗАДАЧА\nПридумай тему для следующего поста.\nТолько JSON, без markdown."
                    .to_string(),
            )),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }],
        stream: false,
        n: None,
        max_tokens: None, // Client did not set max_tokens
        temperature: Some(1.0),
        top_p: Some(0.0), // Client sent top_p=0.0
        stop: None,
        response_format: Some(ResponseFormat { r#type: "json_object".to_string() }),
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        instructions: None,
        input: None,
        prompt: None,
        size: None,
        quality: None,
        person_generation: None,
    };

    // mapped_model for gemini-3-pro-high is gemini-3-pro-preview
    let mapped_model = "gemini-3-pro-preview";
    let result = transform_openai_request(&req, "test-project", mapped_model);

    let gen_config = &result["request"]["generationConfig"];

    // 1. thinkingConfig MUST be injected for gemini-3-pro models
    assert!(
        gen_config.get("thinkingConfig").is_some(),
        "thinkingConfig must be injected for gemini-3-pro-preview (thinking model)"
    );
    let thinking_config = &gen_config["thinkingConfig"];
    assert!(thinking_config["includeThoughts"].as_bool().unwrap(), "includeThoughts must be true");
    assert!(
        thinking_config.get("thinkingBudget").is_some(),
        "thinkingBudget is required by cloudcode API"
    );

    // 2. maxOutputTokens must be set and > thinkingBudget
    let max_output = gen_config["maxOutputTokens"].as_i64().unwrap();
    let thinking_budget = thinking_config["thinkingBudget"].as_i64().unwrap();
    assert!(
        max_output > thinking_budget,
        "maxOutputTokens ({}) must be > thinkingBudget ({})",
        max_output,
        thinking_budget
    );

    // 3. maxOutputTokens must not be excessively large (48768 is safe upper bound for thinking)
    // Upstream uses budget + 32768 as default overhead
    assert!(max_output <= 65536, "maxOutputTokens ({}) should not exceed 65536", max_output);

    // 4. topP must NOT be 0.0 (Gemini API rejects this value)
    let top_p = gen_config["topP"].as_f64().unwrap();
    assert!(top_p > 0.0, "topP ({}) must be > 0.0 — Gemini API rejects 0.0", top_p);

    // 5. responseMimeType should be set for json_object
    assert_eq!(
        gen_config["responseMimeType"].as_str().unwrap(),
        "application/json",
        "responseMimeType must be set for json_object response_format"
    );

    // 6. final_model must be gemini-3-pro-high (physical name, not preview alias)
    let final_model = result["model"].as_str().unwrap();
    assert_eq!(
        final_model, "gemini-3-pro-high",
        "final_model must be remapped from preview to physical name"
    );
}
