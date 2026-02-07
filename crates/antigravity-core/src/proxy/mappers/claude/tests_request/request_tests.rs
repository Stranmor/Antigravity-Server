// Tests for claude/request.rs
use crate::proxy::common::json_schema::clean_json_schema;
use crate::proxy::mappers::claude::models::*;
use crate::proxy::mappers::claude::request::*;
use serde_json::json;

#[test]
fn test_ephemeral_injection_debug() {
    // This test simulates the issue where cache_control might be injected
    let json_with_null = json!({
        "model": "claude-3-5-sonnet-20241022",
        "messages": [
            {
                "role": "assistant",
                "content": [
                    {
                        "type": "thinking",
                        "thinking": "test",
                        "signature": "sig_1234567890",
                        "cache_control": null
                    }
                ]
            }
        ]
    });

    let req: ClaudeRequest = serde_json::from_value(json_with_null).unwrap();
    if let MessageContent::Array(blocks) = &req.messages[0].content {
        if let ContentBlock::Thinking { cache_control, .. } = &blocks[0] {
            assert!(
                cache_control.is_none(),
                "Deserialization should result in None for null cache_control"
            );
        }
    }

    // Now test serialization
    let serialized = serde_json::to_value(&req).unwrap();
    println!("Serialized: {}", serialized);
    assert!(serialized["messages"][0]["content"][0].get("cache_control").is_none());
}

#[test]
fn test_simple_request() {
    let req = ClaudeRequest {
        model: "claude-sonnet-4-5".to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: MessageContent::String("Hello".to_string()),
        }],
        system: None,
        tools: None,
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        top_k: None,
        thinking: None,
        metadata: None,
        output_config: None,
    };

    let result = transform_claude_request_in(&req, "test-project", false);
    assert!(result.is_ok());

    let body = result.unwrap();
    assert_eq!(body["project"], "test-project");
    assert!(body["requestId"].as_str().unwrap().starts_with("agent-"));
}

#[test]
fn test_clean_json_schema() {
    let mut schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "location": {
                "type": "string",
                "description": "The city and state, e.g. San Francisco, CA",
                "minLength": 1,
                "exclusiveMinimum": 0
            },
            "unit": {
                "type": ["string", "null"],
                "enum": ["celsius", "fahrenheit"],
                "default": "celsius"
            },
            "date": {
                "type": "string",
                "format": "date"
            }
        },
        "required": ["location"]
    });

    clean_json_schema(&mut schema);

    // Check removed fields
    assert!(schema.get("$schema").is_none());
    assert!(schema.get("additionalProperties").is_none());
    assert!(schema["properties"]["location"].get("minLength").is_none());
    assert!(schema["properties"]["unit"].get("default").is_none());
    assert!(schema["properties"]["date"].get("format").is_none());

    // Check union type handling ["string", "null"] -> "string"
    assert_eq!(schema["properties"]["unit"]["type"], "string");

    // Check types are lowercased
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["location"]["type"], "string");
    assert_eq!(schema["properties"]["date"]["type"], "string");
}

#[test]
fn test_complex_tool_result() {
    let req = ClaudeRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![
            Message {
                role: "user".to_string(),
                content: MessageContent::String("Run command".to_string()),
            },
            Message {
                role: "assistant".to_string(),
                content: MessageContent::Array(vec![ContentBlock::ToolUse {
                    id: "call_1".to_string(),
                    name: "run_command".to_string(),
                    input: json!({"command": "ls"}),
                    signature: None,
                    cache_control: None,
                }]),
            },
            Message {
                role: "user".to_string(),
                content: MessageContent::Array(vec![ContentBlock::ToolResult {
                    tool_use_id: "call_1".to_string(),
                    content: json!([
                        {"type": "text", "text": "file1.txt\n"},
                        {"type": "text", "text": "file2.txt"}
                    ]),
                    is_error: Some(false),
                }]),
            },
        ],
        system: None,
        tools: None,
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        top_k: None,
        thinking: None,
        metadata: None,
        output_config: None,
    };

    let result = transform_claude_request_in(&req, "test-project", false);
    assert!(result.is_ok());

    let body = result.unwrap();
    let contents = body["request"]["contents"].as_array().unwrap();

    // Check the tool result message (last message)
    let tool_resp_msg = &contents[2];
    let parts = tool_resp_msg["parts"].as_array().unwrap();
    let func_resp = &parts[0]["functionResponse"];

    assert_eq!(func_resp["name"], "run_command");
    assert_eq!(func_resp["id"], "call_1");

    // Verify merged content
    let resp_text = func_resp["response"]["result"].as_str().unwrap();
    assert!(resp_text.contains("file1.txt"));
    assert!(resp_text.contains("file2.txt"));
    assert!(resp_text.contains("\n"));
}

#[test]
fn test_cache_control_cleanup() {
    // simulate VS Code pluginsend containing cache_control  historymessage
    let req = ClaudeRequest {
        model: "claude-sonnet-4-5".to_string(),
        messages: vec![
            Message {
                role: "user".to_string(),
                content: MessageContent::String("Hello".to_string()),
            },
            Message {
                role: "assistant".to_string(),
                content: MessageContent::Array(vec![
                    ContentBlock::Thinking {
                        thinking: "Let me think...".to_string(),
                        signature: Some("sig123".to_string()),
                        cache_control: Some(json!({"type": "ephemeral"})), // thisshouldbecleanup
                    },
                    ContentBlock::Text { text: "Here is my response".to_string() },
                ]),
            },
            Message {
                role: "user".to_string(),
                content: MessageContent::Array(vec![ContentBlock::Image {
                    source: ImageSource {
                        source_type: "base64".to_string(),
                        media_type: "image/png".to_string(),
                        data: "iVBORw0KGgo=".to_string(),
                    },
                    cache_control: Some(json!({"type": "ephemeral"})), // thisalsoshouldbecleanup
                }]),
            },
        ],
        system: None,
        tools: None,
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        top_k: None,
        thinking: None,
        metadata: None,
        output_config: None,
    };

    let result = transform_claude_request_in(&req, "test-project", false);
    assert!(result.is_ok());

    // verifyrequestsuccessconvert
    let body = result.unwrap();
    assert_eq!(body["project"], "test-project");

    // note: cache_control  cleanuphappens ininternal,wecannotdirectlyfrom JSON outputverify
    // but ifdoes not havecleanup,subsequentlysend to Anthropic API whenwill error
    // thistestmainlyensurecleanuplogicwill notcauseconvertfailed
}

#[test]
fn test_thinking_stays_enabled_with_tool_use_history() {
    // [scenario] History contains a tool call chain, assistant message has no thinking block.
    // Expected: thinkingConfig stays enabled. Auto-disable was removed (2026-02-07)
    // to prevent infinite degradation loop. Upstream API handles mixed tool-use/thinking natively.
    let req = ClaudeRequest {
        model: "claude-sonnet-4-5".to_string(),
        messages: vec![
            Message {
                role: "user".to_string(),
                content: MessageContent::String("Check files".to_string()),
            },
            // Assistant uses tool without a thinking block
            Message {
                role: "assistant".to_string(),
                content: MessageContent::Array(vec![
                    ContentBlock::Text { text: "Checking...".to_string() },
                    ContentBlock::ToolUse {
                        id: "tool_1".to_string(),
                        name: "list_files".to_string(),
                        input: json!({}),
                        cache_control: None,
                        signature: None,
                    },
                ]),
            },
            // User returns tool result
            Message {
                role: "user".to_string(),
                content: MessageContent::Array(vec![ContentBlock::ToolResult {
                    tool_use_id: "tool_1".to_string(),
                    content: serde_json::Value::String("file1.txt\nfile2.txt".to_string()),
                    is_error: Some(false),
                }]),
            },
        ],
        system: None,
        tools: Some(vec![Tool {
            name: Some("list_files".to_string()),
            description: Some("List files".to_string()),
            input_schema: Some(json!({"type": "object"})),
            type_: None,
        }]),
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        top_k: None,
        thinking: Some(ThinkingConfig { type_: "enabled".to_string(), budget_tokens: Some(1024) }),
        metadata: None,
        output_config: None,
    };

    let result = transform_claude_request_in(&req, "test-project", false);
    assert!(result.is_ok());

    let body = result.unwrap();
    let request = &body["request"];

    // Verify: thinkingConfig is present even with tool_use in history
    // (auto-disable was removed to prevent degradation loop)
    if let Some(gen_config) = request.get("generationConfig") {
        assert!(
            gen_config.get("thinkingConfig").is_some(),
            "thinkingConfig should remain enabled — auto-disable was removed"
        );
    }

    // Verify: request body is still valid
    assert!(request.get("contents").is_some());
}

#[test]
fn test_thinking_block_not_prepend_when_disabled() {
    // verifywhen thinking not yetenablewhen,will notcomplete thinking block
    let req = ClaudeRequest {
        model: "claude-sonnet-4-5".to_string(),
        messages: vec![
            Message {
                role: "user".to_string(),
                content: MessageContent::String("Hello".to_string()),
            },
            Message {
                role: "assistant".to_string(),
                content: MessageContent::Array(vec![ContentBlock::Text {
                    text: "Response".to_string(),
                }]),
            },
        ],
        system: None,
        tools: None,
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        top_k: None,
        thinking: None, // not yetenable thinking
        metadata: None,
        output_config: None,
    };

    let result = transform_claude_request_in(&req, "test-project", false);
    assert!(result.is_ok());

    let body = result.unwrap();
    let contents = body["request"]["contents"].as_array().unwrap();

    let last_model_msg = contents.iter().rev().find(|c| c["role"] == "model").unwrap();

    let parts = last_model_msg["parts"].as_array().unwrap();

    // verifydoes not havecomplete thinking block
    assert_eq!(parts.len(), 1, "Should only have the original text block");
    assert_eq!(parts[0]["text"], "Response");
}

#[test]
fn test_thinking_block_empty_content_fix() {
    // [scenario] client sends a thinking block with empty content
    // expected: autopad "..." and preserve thought: true
    // Uses Gemini model to avoid Claude-specific signature stripping
    let req = ClaudeRequest {
        model: "gemini-3-pro".to_string(),
        messages: vec![Message {
            role: "assistant".to_string(),
            content: MessageContent::Array(vec![
                ContentBlock::Thinking {
                    thinking: "".to_string(), // emptycontent
                    signature: Some("sig".to_string()),
                    cache_control: None,
                },
                ContentBlock::Text { text: "Hi".to_string() },
            ]),
        }],
        system: None,
        tools: None,
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        top_k: None,
        thinking: Some(ThinkingConfig { type_: "enabled".to_string(), budget_tokens: Some(1024) }),
        metadata: None,
        output_config: None,
    };

    let result = transform_claude_request_in(&req, "test-project", false);
    assert!(result.is_ok(), "Transformation failed");
    let body = result.unwrap();
    let contents = body["request"]["contents"].as_array().unwrap();
    let parts = contents[0]["parts"].as_array().unwrap();

    // verify thinking block is preserved as thought with dummy signature
    assert_eq!(parts[0]["text"], "...", "Empty thinking should be filled with ...");
    assert_eq!(
        parts[0].get("thought").and_then(|v| v.as_bool()),
        Some(true),
        "Empty thinking should be preserved as thought block with dummy signature"
    );
}

#[test]
fn test_redacted_thinking_degradation() {
    // [scenario] clientcontaining RedactedThinking
    // expected: fallbackasnormaltext，notwith thought: true
    let req = ClaudeRequest {
        model: "claude-sonnet-4-5".to_string(),
        messages: vec![Message {
            role: "assistant".to_string(),
            content: MessageContent::Array(vec![
                ContentBlock::RedactedThinking { data: "some data".to_string() },
                ContentBlock::Text { text: "Hi".to_string() },
            ]),
        }],
        system: None,
        tools: None,
        stream: false,
        max_tokens: None,
        temperature: None,
        top_p: None,
        top_k: None,
        thinking: None,
        metadata: None,
        output_config: None,
    };

    let result = transform_claude_request_in(&req, "test-project", false);
    assert!(result.is_ok());
    let body = result.unwrap();
    let parts = body["request"]["contents"][0]["parts"].as_array().unwrap();

    // verify RedactedThinking -> Text
    let text = parts[0]["text"].as_str().unwrap();
    assert!(text.contains("[Redacted Thinking: some data]"));
    assert!(parts[0].get("thought").is_none(), "Redacted thinking should NOT have thought: true");
}

// ==================================================================================
// [FIX #564] Test: Thinking blocks are sorted to be first after context compression
// ==================================================================================
#[test]
fn test_thinking_blocks_sorted_first_after_compression() {
    // Simulate kilo context compression reordering: text BEFORE thinking
    let mut messages = vec![Message {
        role: "assistant".to_string(),
        content: MessageContent::Array(vec![
            // Wrong order: Text before Thinking (simulates kilo compression)
            ContentBlock::Text { text: "Some regular text".to_string() },
            ContentBlock::Thinking {
                thinking: "My thinking process".to_string(),
                signature: Some(
                    "valid_signature_1234567890_abcdefghij_klmnopqrstuvwxyz_test".to_string(),
                ),
                cache_control: None,
            },
            ContentBlock::Text { text: "More text".to_string() },
        ]),
    }];

    // Apply the fix
    sort_thinking_blocks_first(&mut messages);

    // Verify thinking is now first
    if let MessageContent::Array(blocks) = &messages[0].content {
        assert_eq!(blocks.len(), 3, "Should still have 3 blocks");
        assert!(matches!(blocks[0], ContentBlock::Thinking { .. }), "Thinking should be first");
        assert!(matches!(blocks[1], ContentBlock::Text { .. }), "Text should be second");
        assert!(matches!(blocks[2], ContentBlock::Text { .. }), "Text should be third");

        // Verify content preserved
        if let ContentBlock::Thinking { thinking, .. } = &blocks[0] {
            assert_eq!(thinking, "My thinking process");
        }
    } else {
        panic!("Expected Array content");
    }
}

#[test]
fn test_thinking_blocks_no_reorder_when_already_first() {
    // Correct order: Thinking already first - should not trigger reorder
    let mut messages = vec![Message {
        role: "assistant".to_string(),
        content: MessageContent::Array(vec![
            ContentBlock::Thinking {
                thinking: "My thinking".to_string(),
                signature: Some("sig123".to_string()),
                cache_control: None,
            },
            ContentBlock::Text { text: "Some text".to_string() },
        ]),
    }];

    // Apply the fix (should be no-op)
    sort_thinking_blocks_first(&mut messages);

    // Verify order unchanged
    if let MessageContent::Array(blocks) = &messages[0].content {
        assert!(
            matches!(blocks[0], ContentBlock::Thinking { .. }),
            "Thinking should still be first"
        );
        assert!(matches!(blocks[1], ContentBlock::Text { .. }), "Text should still be second");
    }
}

#[test]
fn test_merge_consecutive_messages() {
    let mut messages = vec![
        Message { role: "user".to_string(), content: MessageContent::String("Hello".to_string()) },
        Message {
            role: "user".to_string(),
            content: MessageContent::Array(vec![ContentBlock::Text { text: "World".to_string() }]),
        },
        Message {
            role: "assistant".to_string(),
            content: MessageContent::String("Hi".to_string()),
        },
        Message {
            role: "user".to_string(),
            content: MessageContent::Array(vec![ContentBlock::ToolResult {
                tool_use_id: "test_id".to_string(),
                content: serde_json::json!("result"),
                is_error: None,
            }]),
        },
        Message {
            role: "user".to_string(),
            content: MessageContent::Array(vec![ContentBlock::Text {
                text: "System Reminder".to_string(),
            }]),
        },
    ];

    merge_consecutive_messages(&mut messages);

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].role, "user");
    if let MessageContent::Array(blocks) = &messages[0].content {
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Hello"),
            _ => panic!("Expected text block"),
        }
        match &blocks[1] {
            ContentBlock::Text { text } => assert_eq!(text, "World"),
            _ => panic!("Expected text block"),
        }
    } else {
        panic!("Expected array content at index 0");
    }

    assert_eq!(messages[1].role, "assistant");

    assert_eq!(messages[2].role, "user");
    if let MessageContent::Array(blocks) = &messages[2].content {
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::ToolResult { tool_use_id, .. } => assert_eq!(tool_use_id, "test_id"),
            _ => panic!("Expected tool_result block"),
        }
        match &blocks[1] {
            ContentBlock::Text { text } => assert_eq!(text, "System Reminder"),
            _ => panic!("Expected text block"),
        }
    } else {
        panic!("Expected array content at index 2");
    }
}
