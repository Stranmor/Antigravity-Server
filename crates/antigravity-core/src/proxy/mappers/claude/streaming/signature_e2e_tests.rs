#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "test code")]
mod tests {
    use base64::Engine;
    use serde_json::json;

    use crate::proxy::mappers::claude::models::{FunctionCall, GeminiPart};
    use crate::proxy::mappers::claude::request::signature_validator;
    use crate::proxy::mappers::claude::request::DUMMY_SIGNATURE;
    use crate::proxy::mappers::claude::streaming::{PartProcessor, StreamingState};
    use crate::proxy::SignatureCache;

    // Must be ≥50 chars to pass MIN_SIGNATURE_LENGTH checks in cache
    const RAW_SIG: &str = "e2e_test_signature_value_long_enough_for_min_length_check_padding_ok";

    fn b64(raw: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(raw)
    }

    fn make_thinking_part(text: &str, sig: Option<String>) -> GeminiPart {
        GeminiPart {
            text: Some(text.to_string()),
            thought: Some(true),
            thought_signature: sig,
            function_call: None,
            function_response: None,
            inline_data: None,
        }
    }

    fn chunks_to_string(chunks: &[bytes::Bytes]) -> String {
        chunks.iter().map(|b| String::from_utf8(b.to_vec()).unwrap()).collect::<Vec<_>>().join("")
    }

    #[test]
    fn test_thinking_signature_emitted_in_sse_output() {
        let mut state = StreamingState::new();
        state.session_id = Some("sig-e2e-test1".to_string());
        state.model_name = Some("gemini-2.5-pro".to_string());

        let part = make_thinking_part("Let me think...", Some(b64(RAW_SIG)));

        let mut chunks = PartProcessor::new(&mut state).process(&part);
        chunks.extend(state.end_block());

        let output = chunks_to_string(&chunks);
        assert!(
            output.contains("signature_delta"),
            "SSE output must contain signature_delta event"
        );
        assert!(output.contains(RAW_SIG), "signature_delta must contain the DECODED signature");
    }

    #[test]
    fn test_signature_cached_in_session_cache() {
        let session = "sig-e2e-test2-session";
        let mut state = StreamingState::new();
        state.session_id = Some(session.to_string());
        state.model_name = Some("gemini-2.5-pro".to_string());

        let part = make_thinking_part("Reasoning about caching", Some(b64(RAW_SIG)));
        let _ = PartProcessor::new(&mut state).process(&part);

        let cached = SignatureCache::global().get_session_signature(session);
        assert_eq!(
            cached.as_deref(),
            Some(RAW_SIG),
            "Session cache must hold the decoded signature"
        );
    }

    #[test]
    fn test_multi_turn_signature_recovery() {
        let thinking_text = "Deep analysis of the problem at hand for multi-turn recovery test";
        let session = "sig-e2e-test3-multi";
        let mut state = StreamingState::new();
        state.session_id = Some(session.to_string());
        state.model_name = Some("gemini-2.5-pro".to_string());

        let part = make_thinking_part(thinking_text, Some(b64(RAW_SIG)));
        let _ = PartProcessor::new(&mut state).process(&part);
        let _ = state.end_block();

        let mut last_sig: Option<String> = None;
        let action = signature_validator::validate_thinking_signature(
            thinking_text,
            None,
            false,
            "gemini-2.5-pro",
            &mut last_sig,
        );

        let signature_validator::SignatureAction::UseWithSignature { part } = action;
        let sig_in_part = part.get("thoughtSignature").and_then(|v| v.as_str()).unwrap();
        assert_eq!(sig_in_part, RAW_SIG, "Content cache must recover the original signature");
    }

    #[test]
    fn test_tool_call_with_signature_preserved() {
        let mut state = StreamingState::new();
        state.session_id = Some("sig-e2e-test4-tool".to_string());
        state.model_name = Some("gemini-2.5-pro".to_string());

        let fc = FunctionCall {
            name: "read_file".to_string(),
            args: Some(json!({"path": "/tmp/test"})),
            id: Some("tool_sig_e2e_4".to_string()),
        };
        let part = GeminiPart {
            text: None,
            thought: None,
            thought_signature: Some(b64(RAW_SIG)),
            function_call: Some(fc),
            function_response: None,
            inline_data: None,
        };

        let chunks = PartProcessor::new(&mut state).process(&part);
        let output = chunks_to_string(&chunks);

        assert!(
            output.contains("content_block_start"),
            "Must emit content_block_start for tool_use"
        );
        assert!(
            output.contains("\"signature\""),
            "content_block_start must include signature field"
        );
        assert!(output.contains(RAW_SIG), "signature field must contain decoded signature value");
    }

    #[test]
    fn test_missing_signature_uses_dummy() {
        let thinking_text = "Unique thinking without any signature for dummy test e2e";
        let mut last_sig: Option<String> = None;

        let action = signature_validator::validate_thinking_signature(
            thinking_text,
            None,
            false,
            "gemini-2.5-pro",
            &mut last_sig,
        );

        let signature_validator::SignatureAction::UseWithSignature { part } = action;
        let sig = part.get("thoughtSignature").and_then(|v| v.as_str()).unwrap();
        assert_eq!(sig, DUMMY_SIGNATURE, "Missing signature must fall back to DUMMY_SIGNATURE");
    }

    #[test]
    fn test_full_sse_line_signature_flow() {
        let session = "sig-e2e-test6-full-flow";
        let mut state = StreamingState::new();
        state.session_id = Some(session.to_string());
        state.model_name = Some("gemini-2.5-pro".to_string());

        let gemini_json = json!({
            "text": "Planning the implementation carefully with full analysis",
            "thought": true,
            "thoughtSignature": b64(RAW_SIG)
        });
        let part: GeminiPart = serde_json::from_value(gemini_json).unwrap();

        let mut chunks = PartProcessor::new(&mut state).process(&part);
        chunks.extend(state.end_block());
        let output = chunks_to_string(&chunks);

        assert!(output.contains("signature_delta"), "Full flow must emit signature_delta in SSE");
        assert!(output.contains(RAW_SIG), "signature_delta must contain decoded signature");

        let cached = SignatureCache::global().get_session_signature(session);
        assert_eq!(
            cached.as_deref(),
            Some(RAW_SIG),
            "Session cache must be populated after full flow"
        );
    }

    #[test]
    fn test_trailing_signature_emitted_on_finish() {
        let session = "sig-e2e-trailing-finish";
        let mut state = StreamingState::new();
        state.session_id = Some(session.to_string());
        state.model_name = Some("gemini-2.5-pro".to_string());

        let part = make_thinking_part("Some thinking without sig", None);
        let _ = PartProcessor::new(&mut state).process(&part);

        state.set_trailing_signature(Some(RAW_SIG.to_string()));

        let finish_chunks = state.emit_finish(Some("STOP"), None);
        let output = chunks_to_string(&finish_chunks);

        assert!(
            output.contains("signature_delta"),
            "emit_finish must emit signature_delta for trailing signature"
        );
        assert!(
            output.contains(RAW_SIG),
            "signature_delta must contain the trailing signature value"
        );

        let cached = SignatureCache::global().get_session_signature(session);
        assert_eq!(
            cached.as_deref(),
            Some(RAW_SIG),
            "Trailing signature must be cached in SignatureCache"
        );
    }

    #[test]
    fn test_text_part_with_signature_caches_session() {
        let session = "sig-e2e-text-cache";
        let mut state = StreamingState::new();
        state.session_id = Some(session.to_string());
        state.model_name = Some("gemini-2.5-pro".to_string());

        let part = GeminiPart {
            text: Some("Regular text with sig".to_string()),
            thought: Some(false),
            thought_signature: Some(b64(RAW_SIG)),
            function_call: None,
            function_response: None,
            inline_data: None,
        };

        let _ = PartProcessor::new(&mut state).process(&part);

        let cached = SignatureCache::global().get_session_signature(session);
        assert_eq!(
            cached.as_deref(),
            Some(RAW_SIG),
            "Text part with signature must cache to SignatureCache"
        );
    }
}
