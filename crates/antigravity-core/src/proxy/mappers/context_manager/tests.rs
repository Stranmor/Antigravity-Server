use super::*;
use crate::proxy::mappers::claude::models::{ClaudeRequest, ContentBlock, Message, MessageContent};

fn create_test_request() -> ClaudeRequest {
    ClaudeRequest {
        model: "claude-3-5-sonnet".into(),
        messages: vec![],
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
    }
}

#[test]
fn test_estimate_tokens() {
    let mut req = create_test_request();
    req.messages = vec![Message {
        role: "user".into(),
        content: MessageContent::String("Hello World".into()),
    }];

    let tokens = ContextManager::estimate_token_usage(&req);
    assert!(tokens > 0);
    assert!(tokens < 50);
}

#[test]
fn test_purify_history_soft() {
    let mut messages = vec![
        Message {
            role: "assistant".into(),
            content: MessageContent::Array(vec![
                ContentBlock::Thinking {
                    thinking: "ancient".into(),
                    signature: None,
                    cache_control: None,
                },
                ContentBlock::Text { text: "A0".into() },
            ]),
        },
        Message { role: "user".into(), content: MessageContent::String("Q1".into()) },
        Message {
            role: "assistant".into(),
            content: MessageContent::Array(vec![
                ContentBlock::Thinking {
                    thinking: "old".into(),
                    signature: None,
                    cache_control: None,
                },
                ContentBlock::Text { text: "A1".into() },
            ]),
        },
        Message { role: "user".into(), content: MessageContent::String("Q2".into()) },
        Message {
            role: "assistant".into(),
            content: MessageContent::Array(vec![
                ContentBlock::Thinking {
                    thinking: "recent".into(),
                    signature: None,
                    cache_control: None,
                },
                ContentBlock::Text { text: "A2".into() },
            ]),
        },
        Message { role: "user".into(), content: MessageContent::String("current".into()) },
    ];

    ContextManager::purify_history(&mut messages, PurificationStrategy::Soft);

    if let MessageContent::Array(blocks) = &messages[0].content {
        assert_eq!(blocks.len(), 1);
        if let ContentBlock::Text { text } = &blocks[0] {
            assert_eq!(text, "A0");
        } else {
            panic!("Wrong block");
        }
    }

    if let MessageContent::Array(blocks) = &messages[2].content {
        assert_eq!(blocks.len(), 2);
    }
}

#[test]
fn test_purify_history_aggressive() {
    let mut messages = vec![Message {
        role: "assistant".into(),
        content: MessageContent::Array(vec![
            ContentBlock::Thinking {
                thinking: "thought".into(),
                signature: None,
                cache_control: None,
            },
            ContentBlock::Text { text: "text".into() },
        ]),
    }];

    ContextManager::purify_history(&mut messages, PurificationStrategy::Aggressive);

    if let MessageContent::Array(blocks) = &messages[0].content {
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], ContentBlock::Text { .. }));
    }
}
