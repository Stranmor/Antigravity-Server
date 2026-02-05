use super::super::response::transform_response;
use crate::proxy::mappers::claude::models::*;

#[test]
fn test_simple_text_response() {
    let gemini_resp = GeminiResponse {
        candidates: Some(vec![Candidate {
            content: Some(GeminiContent {
                role: "model".to_string(),
                parts: vec![GeminiPart {
                    text: Some("Hello, world!".to_string()),
                    thought: None,
                    thought_signature: None,
                    function_call: None,
                    function_response: None,
                    inline_data: None,
                }],
            }),
            finish_reason: Some("STOP".to_string()),
            index: Some(0),
            grounding_metadata: None,
        }]),
        usage_metadata: Some(UsageMetadata {
            prompt_token_count: Some(10),
            candidates_token_count: Some(5),
            total_token_count: Some(15),
            cached_content_token_count: None,
        }),
        model_version: Some("gemini-2.5-flash".to_string()),
        response_id: Some("resp_123".to_string()),
    };

    let result =
        transform_response(&gemini_resp, false, 1_000_000, None, "gemini-2.5-flash".to_string());
    assert!(result.is_ok());

    let claude_resp = result.unwrap();
    assert_eq!(claude_resp.role, "assistant");
    assert_eq!(claude_resp.stop_reason, "end_turn");
    assert_eq!(claude_resp.content.len(), 1);

    match &claude_resp.content[0] {
        ContentBlock::Text { text } => {
            assert_eq!(text, "Hello, world!");
        },
        _ => panic!("Expected Text block"),
    }
}

#[test]
fn test_thinking_with_signature() {
    let gemini_resp = GeminiResponse {
        candidates: Some(vec![Candidate {
            content: Some(GeminiContent {
                role: "model".to_string(),
                parts: vec![
                    GeminiPart {
                        text: Some("Let me think...".to_string()),
                        thought: Some(true),
                        thought_signature: Some("sig123".to_string()),
                        function_call: None,
                        function_response: None,
                        inline_data: None,
                    },
                    GeminiPart {
                        text: Some("The answer is 42".to_string()),
                        thought: None,
                        thought_signature: None,
                        function_call: None,
                        function_response: None,
                        inline_data: None,
                    },
                ],
            }),
            finish_reason: Some("STOP".to_string()),
            index: Some(0),
            grounding_metadata: None,
        }]),
        usage_metadata: None,
        model_version: Some("gemini-2.5-flash".to_string()),
        response_id: Some("resp_456".to_string()),
    };

    let result =
        transform_response(&gemini_resp, false, 1_000_000, None, "gemini-2.5-flash".to_string());
    assert!(result.is_ok());

    let claude_resp = result.unwrap();
    assert_eq!(claude_resp.content.len(), 2);

    match &claude_resp.content[0] {
        ContentBlock::Thinking { thinking, signature, .. } => {
            assert_eq!(thinking, "Let me think...");
            assert_eq!(signature.as_deref(), Some("sig123"));
        },
        _ => panic!("Expected Thinking block"),
    }

    match &claude_resp.content[1] {
        ContentBlock::Text { text } => {
            assert_eq!(text, "The answer is 42");
        },
        _ => panic!("Expected Text block"),
    }
}
