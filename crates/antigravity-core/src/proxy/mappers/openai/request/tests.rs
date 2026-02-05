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
