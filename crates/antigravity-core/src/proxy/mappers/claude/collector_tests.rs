use super::collector::collect_stream_to_json;
use super::models::ContentBlock;
use bytes::Bytes;
use futures::stream;
use std::io;

#[tokio::test]
async fn test_collect_simple_text_response() {
    let sse_data = vec![
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_123\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-5-sonnet\",\"content\":[],\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" World\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ];

    let byte_stream =
        stream::iter(sse_data.into_iter().map(|s| Ok::<Bytes, io::Error>(Bytes::from(s))));

    let result = collect_stream_to_json(byte_stream).await;
    assert!(result.is_ok());

    let response = result.unwrap();
    assert_eq!(response.id, "msg_123");
    assert_eq!(response.model, "claude-3-5-sonnet");
    assert_eq!(response.content.len(), 1);

    if let ContentBlock::Text { text } = &response.content[0] {
        assert_eq!(text, "Hello World");
    } else {
        panic!("Expected Text block");
    }
}

#[tokio::test]
async fn test_collect_thinking_response_with_signature() {
    let sse_data = vec![
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_think\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"claude-3-7-sonnet\",\"content\":[],\"stop_reason\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\", \"signature\": \"sig_123456\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"I am \"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"thinking\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":10}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ];

    let byte_stream =
        stream::iter(sse_data.into_iter().map(|s| Ok::<Bytes, io::Error>(Bytes::from(s))));

    let result = collect_stream_to_json(byte_stream).await;
    assert!(result.is_ok());

    let response = result.unwrap();

    if let ContentBlock::Thinking { thinking, signature, .. } = &response.content[0] {
        assert_eq!(thinking, "I am thinking");
        assert_eq!(signature.as_deref(), Some("sig_123456"));
    } else {
        panic!("Expected Thinking block");
    }
}
