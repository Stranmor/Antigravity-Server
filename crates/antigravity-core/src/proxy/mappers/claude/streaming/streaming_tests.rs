#[cfg(test)]
mod tests {
    use crate::proxy::mappers::claude::streaming::{
        BlockType, PartProcessor, SignatureManager, StreamingState,
    };
    use serde_json::json;

    #[test]
    fn test_signature_manager() {
        let mut mgr = SignatureManager::new();
        assert!(!mgr.has_pending());

        mgr.store(Some("sig123".to_string()));
        assert!(mgr.has_pending());

        let sig = mgr.consume();
        assert_eq!(sig, Some("sig123".to_string()));
        assert!(!mgr.has_pending());
    }

    #[test]
    fn test_streaming_state_emit() {
        let state = StreamingState::new();
        let chunk = state.emit("test_event", json!({"foo": "bar"}));

        let s = String::from_utf8(chunk.to_vec()).unwrap();
        assert!(s.contains("event: test_event"));
        assert!(s.contains("\"foo\":\"bar\""));
    }

    #[test]
    fn test_process_function_call_deltas() {
        use crate::proxy::mappers::claude::models::{FunctionCall, GeminiPart};

        let mut state = StreamingState::new();
        let mut processor = PartProcessor::new(&mut state);

        let fc = FunctionCall {
            name: "test_tool".to_string(),
            args: Some(json!({"arg": "value"})),
            id: Some("call_123".to_string()),
        };

        let part = GeminiPart {
            text: None,
            function_call: Some(fc),
            inline_data: None,
            thought: None,
            thought_signature: None,
            function_response: None,
        };

        let chunks = processor.process(&part);
        let output = chunks
            .iter()
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .collect::<Vec<_>>()
            .join("");

        assert!(output.contains(r#""type":"content_block_start""#));
        assert!(output.contains(r#""name":"test_tool""#));
        assert!(output.contains(r#""input":{}"#));

        assert!(output.contains(r#""type":"content_block_delta""#));
        assert!(output.contains(r#""type":"input_json_delta""#));
        assert!(output.contains(r#"partial_json":"{\"arg\":\"value\"}"#));

        assert!(output.contains(r#""type":"content_block_stop""#));
    }

    #[test]
    fn test_truncation_detection_inside_text_block() {
        let mut state = StreamingState::new();
        state.message_start_sent = true;
        state.block_type = BlockType::Text;

        let chunks = state.emit_finish(None, None);
        let output = chunks
            .iter()
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .collect::<Vec<_>>()
            .join("");

        assert!(output.contains(r#""stop_reason":"max_tokens""#));
    }

    #[test]
    fn test_truncation_detection_inside_function_block() {
        let mut state = StreamingState::new();
        state.message_start_sent = true;
        state.block_type = BlockType::Function;

        let chunks = state.emit_finish(None, None);
        let output = chunks
            .iter()
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .collect::<Vec<_>>()
            .join("");

        assert!(output.contains(r#""stop_reason":"max_tokens""#));
    }

    #[test]
    fn test_no_truncation_when_block_closed() {
        let mut state = StreamingState::new();
        state.message_start_sent = true;
        state.block_type = BlockType::None;

        let chunks = state.emit_finish(None, None);
        let output = chunks
            .iter()
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .collect::<Vec<_>>()
            .join("");

        assert!(output.contains(r#""stop_reason":"end_turn""#));
    }

    #[test]
    fn test_explicit_max_tokens_finish_reason() {
        let mut state = StreamingState::new();
        state.message_start_sent = true;
        state.block_type = BlockType::None;

        let chunks = state.emit_finish(Some("MAX_TOKENS"), None);
        let output = chunks
            .iter()
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .collect::<Vec<_>>()
            .join("");

        assert!(output.contains(r#""stop_reason":"max_tokens""#));
    }
}
