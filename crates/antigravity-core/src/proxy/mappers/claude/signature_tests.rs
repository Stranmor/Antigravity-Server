//! Integration tests for signature handling in the proxy.
//!
//! These tests verify that:
//! 1. Signatures are treated as opaque — never decoded or modified
//! 2. Signatures are cross-platform compatible — no model family filtering
//! 3. Dummy signatures are properly recognized and passed through

#[cfg(test)]
mod tests {
    use serde_json::json;

    // ==================== Signature Opaqueness Tests ====================

    /// Signatures must be passed through exactly as received — no base64 decoding.
    /// This was the root cause of signature validation failures: the proxy was
    /// decoding base64 signatures from Gemini, then sending decoded (corrupted)
    /// values to clients, which would then fail on the next request.
    #[test]
    fn test_signature_passthrough_no_decode() {
        // Simulate a base64-encoded signature from Gemini API
        let opaque_signature =
            "dGhpcyBpcyBhIHRlc3Qgc2lnbmF0dXJlIHRoYXQgc2hvdWxkIG5vdCBiZSBkZWNvZGVk".to_string();

        // The signature should be passed through as-is, no decoding
        let gemini_part = json!({
            "text": "Let me think about this...",
            "thought": true,
            "thoughtSignature": opaque_signature
        });

        let sig = gemini_part["thoughtSignature"].as_str().unwrap();
        assert_eq!(sig, opaque_signature, "Signature must not be modified");
        assert!(sig.len() >= 50, "Opaque signature should be long enough to pass validation");
    }

    /// Verify that the decode_signature function exists but should NOT be used
    /// for thought signatures (it's only for grounding metadata).
    #[test]
    fn test_base64_decode_changes_value() {
        use base64::Engine;
        let original = "dGhpcyBpcyBhIHRlc3Qgc2lnbmF0dXJl";
        let decoded = base64::engine::general_purpose::STANDARD.decode(original).unwrap();
        let decoded_str = String::from_utf8(decoded).unwrap();

        // Decoding CHANGES the value — this is why we must not decode signatures
        assert_ne!(original, decoded_str, "Decoding base64 produces different value");
        assert_eq!(decoded_str, "this is a test signature");
    }

    // ==================== Signature Validity Tests ====================

    #[test]
    fn test_is_valid_or_dummy_signature() {
        use crate::proxy::mappers::claude::request::{DUMMY_SIGNATURE, MIN_SIGNATURE_LENGTH};

        // Dummy signature is always valid — it's the bypass value
        assert_eq!(DUMMY_SIGNATURE, "skip_thought_signature_validator");

        // MIN_SIGNATURE_LENGTH should be a reasonable threshold
        const {
            assert!(MIN_SIGNATURE_LENGTH > 10, "MIN_SIGNATURE_LENGTH should reject very short sigs")
        };
        const { assert!(MIN_SIGNATURE_LENGTH <= 100, "MIN_SIGNATURE_LENGTH should accept real sigs") };
    }

    #[test]
    fn test_dummy_signature_constant() {
        use crate::proxy::mappers::claude::request::signature_validator::DUMMY_SIGNATURE;
        assert_eq!(DUMMY_SIGNATURE, "skip_thought_signature_validator");
    }

    // ==================== Cross-Platform Compatibility Tests ====================

    /// Signatures from Gemini's Vertex API must be accepted for Claude-mapped models.
    /// Per Anthropic docs: "signature values are compatible across platforms
    /// (Claude APIs, Amazon Bedrock, and Vertex AI)."
    #[test]
    fn test_signature_cross_platform_acceptance() {
        use crate::proxy::mappers::claude::request::signature_validator::{
            validate_thinking_signature, SignatureAction,
        };

        let thinking = "Let me analyze this problem...";
        // Simulate a valid-length signature from Gemini/Vertex
        let sig = "a".repeat(200);
        let mut last_sig = None;

        let result = validate_thinking_signature(
            thinking,
            Some(&sig),
            false,
            "claude-opus-4-6-thinking", // target is Claude
            &mut last_sig,
        );

        match result {
            SignatureAction::UseWithSignature { part } => {
                let sig_value = part["thoughtSignature"].as_str().unwrap();
                // Signature must be passed through EXACTLY as received
                assert_eq!(sig_value, sig, "Cross-platform signature must be preserved as-is");
            },
        }
    }

    /// Signatures should work regardless of what mapped_model is.
    #[test]
    fn test_signature_accepted_for_any_model() {
        use crate::proxy::mappers::claude::request::signature_validator::{
            validate_thinking_signature, SignatureAction,
        };

        let thinking = "Deep analysis...";
        let sig = "b".repeat(300);

        for model in &[
            "claude-opus-4-6-thinking",
            "gemini-2.5-flash",
            "claude-sonnet-4-5",
            "gemini-3-pro-preview",
        ] {
            let mut last_sig = None;
            let result =
                validate_thinking_signature(thinking, Some(&sig), false, model, &mut last_sig);

            match result {
                SignatureAction::UseWithSignature { part } => {
                    let sig_value = part["thoughtSignature"].as_str().unwrap();
                    assert_eq!(sig_value, sig, "Signature must be preserved for model {model}");
                    assert_eq!(last_sig.as_deref(), Some(sig.as_str()));
                },
            }
        }
    }

    /// Dummy signatures should be passed through as-is.
    #[test]
    fn test_dummy_signature_passthrough() {
        use crate::proxy::mappers::claude::request::signature_validator::{
            validate_thinking_signature, SignatureAction, DUMMY_SIGNATURE,
        };

        let thinking = "Planning...";
        let mut last_sig = None;

        let result = validate_thinking_signature(
            thinking,
            Some(&DUMMY_SIGNATURE.to_string()),
            false,
            "gemini-2.5-flash",
            &mut last_sig,
        );

        match result {
            SignatureAction::UseWithSignature { part } => {
                let sig_value = part["thoughtSignature"].as_str().unwrap();
                assert_eq!(sig_value, DUMMY_SIGNATURE);
            },
        }
    }

    /// Short/invalid signatures should be replaced with dummy.
    #[test]
    fn test_short_signature_uses_dummy() {
        use crate::proxy::mappers::claude::request::signature_validator::{
            validate_thinking_signature, SignatureAction, DUMMY_SIGNATURE,
        };

        let thinking = "Short sig test...";
        let short_sig = "too_short".to_string();
        let mut last_sig = None;

        let result = validate_thinking_signature(
            thinking,
            Some(&short_sig),
            false,
            "gemini-2.5-flash",
            &mut last_sig,
        );

        match result {
            SignatureAction::UseWithSignature { part } => {
                let sig_value = part["thoughtSignature"].as_str().unwrap();
                assert_eq!(
                    sig_value, DUMMY_SIGNATURE,
                    "Short signature should be replaced with dummy"
                );
            },
        }
    }

    /// No signature at all should use dummy.
    #[test]
    fn test_no_signature_uses_dummy() {
        use crate::proxy::mappers::claude::request::signature_validator::{
            validate_thinking_signature, SignatureAction, DUMMY_SIGNATURE,
        };

        let thinking = "No sig provided...";
        let mut last_sig = None;

        let result =
            validate_thinking_signature(thinking, None, false, "gemini-2.5-flash", &mut last_sig);

        match result {
            SignatureAction::UseWithSignature { part } => {
                let sig_value = part["thoughtSignature"].as_str().unwrap();
                assert_eq!(sig_value, DUMMY_SIGNATURE, "Missing signature should use dummy");
            },
        }
    }

    /// Tool signatures should be accepted regardless of model family.
    #[test]
    fn test_tool_signature_cross_platform() {
        use crate::proxy::mappers::claude::request::signature_validator::should_use_tool_signature;

        let valid_sig = "a".repeat(200);

        // Should accept valid signature regardless of model
        assert!(should_use_tool_signature(
            &valid_sig,
            "tool_123",
            "claude-opus-4-6-thinking",
            true
        ));
        assert!(should_use_tool_signature(&valid_sig, "tool_456", "gemini-2.5-flash", true));
        assert!(should_use_tool_signature(&valid_sig, "tool_789", "claude-sonnet-4-5", false));
    }

    /// Tool signatures that are too short should be rejected.
    #[test]
    fn test_tool_signature_short_rejected() {
        use crate::proxy::mappers::claude::request::signature_validator::should_use_tool_signature;

        assert!(!should_use_tool_signature("short", "tool_123", "claude-opus-4-6-thinking", true));
        assert!(!should_use_tool_signature("", "tool_456", "gemini-2.5-flash", true));
    }

    /// Dummy tool signatures should always be accepted.
    #[test]
    fn test_tool_dummy_signature_accepted() {
        use crate::proxy::mappers::claude::request::signature_validator::{
            should_use_tool_signature, DUMMY_SIGNATURE,
        };

        assert!(should_use_tool_signature(
            DUMMY_SIGNATURE,
            "tool_123",
            "claude-opus-4-6-thinking",
            true
        ));
        assert!(should_use_tool_signature(DUMMY_SIGNATURE, "tool_456", "gemini-2.5-flash", false));
    }
}
