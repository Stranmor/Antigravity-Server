#[cfg(test)]
mod test_fixes {
    use super::super::wrap_request;
    use serde_json::json;

    #[test]
    fn test_wrap_request_with_signature() {
        let session_id = "test-session-sig";
        let signature = "test-signature-must-be-longer-than-fifty-characters-to-be-cached-by-signature-cache-12345";
        crate::proxy::SignatureCache::global()
            .cache_session_signature(session_id, signature.to_string());

        let body = json!({
            "model": "gemini-pro",
            "contents": [{
                "role": "user",
                "parts": [{
                    "functionCall": {
                        "name": "get_weather",
                        "args": {"location": "London"}
                    }
                }]
            }]
        });

        let result = wrap_request(&body, "proj", "gemini-pro", Some(session_id));
        let injected_sig =
            result["request"]["contents"][0]["parts"][0]["thoughtSignature"].as_str().unwrap();
        assert_eq!(injected_sig, signature);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{unwrap_response, wrap_request};
    use serde_json::json;

    #[test]
    fn test_wrap_request() {
        let body = json!({
            "model": "gemini-2.5-flash",
            "contents": [{"role": "user", "parts": [{"text": "Hi"}]}]
        });

        let result = wrap_request(&body, "test-project", "gemini-2.5-flash", None);
        assert_eq!(result["project"], "test-project");
        assert_eq!(result["model"], "gemini-2.5-flash");
        assert!(result["requestId"].as_str().unwrap().starts_with("agent-"));
    }

    #[test]
    fn test_unwrap_response() {
        let wrapped = json!({
            "response": {
                "candidates": [{"content": {"parts": [{"text": "Hello"}]}}]
            }
        });

        let result = unwrap_response(&wrapped);
        assert!(result.get("candidates").is_some());
        assert!(result.get("response").is_none());
    }

    #[test]
    fn test_antigravity_identity_injection_with_role() {
        let body = json!({
            "model": "gemini-pro",
            "messages": []
        });

        let result = wrap_request(&body, "test-proj", "gemini-pro", None);
        let sys = result.get("request").unwrap().get("systemInstruction").unwrap();
        assert_eq!(sys.get("role").unwrap(), "user");
        let parts = sys.get("parts").unwrap().as_array().unwrap();
        assert!(!parts.is_empty());
        let first_text = parts[0].get("text").unwrap().as_str().unwrap();
        assert!(first_text.contains("You are Antigravity"));
    }

    #[test]
    fn test_user_instruction_preservation() {
        let body = json!({
            "model": "gemini-pro",
            "systemInstruction": {
                "role": "user",
                "parts": [{"text": "User custom prompt"}]
            }
        });

        let result = wrap_request(&body, "test-proj", "gemini-pro", None);
        let sys = result.get("request").unwrap().get("systemInstruction").unwrap();
        let parts = sys.get("parts").unwrap().as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].get("text").unwrap().as_str().unwrap().contains("You are Antigravity"));
        assert_eq!(parts[1].get("text").unwrap().as_str().unwrap(), "User custom prompt");
    }

    #[test]
    fn test_duplicate_prevention() {
        let body = json!({
            "model": "gemini-pro",
            "systemInstruction": {
                "parts": [{"text": "You are Antigravity..."}]
            }
        });

        let result = wrap_request(&body, "test-proj", "gemini-pro", None);
        let sys = result.get("request").unwrap().get("systemInstruction").unwrap();
        let parts = sys.get("parts").unwrap().as_array().unwrap();
        assert_eq!(parts.len(), 1);
    }
}

#[cfg(test)]
mod adaptive_tests {
    use super::super::wrap_request;
    use crate::proxy::common::thinking_config::{
        update_thinking_budget_config, THINKING_CONFIG_TEST_LOCK,
    };
    use antigravity_types::models::{ThinkingBudgetConfig, ThinkingBudgetMode};
    use serde_json::json;

    #[test]
    fn test_adaptive_gemini3_replaces_budget_with_level() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Adaptive,
            effort: None,
            ..Default::default()
        });
        let body = json!({
            "model": "gemini-3-pro",
            "contents": [{"role": "user", "parts": [{"text": "Hi"}]}],
            "generationConfig": {
                "thinkingConfig": {
                    "includeThoughts": true,
                    "thinkingBudget": -1
                }
            }
        });
        let result = wrap_request(&body, "proj", "gemini-3-pro", None);
        let tc = &result["request"]["generationConfig"]["thinkingConfig"];
        assert_eq!(tc["thinkingLevel"], "HIGH");
        assert!(tc.get("thinkingBudget").is_none() || tc["thinkingBudget"].is_null());
        assert_eq!(result["request"]["generationConfig"]["maxOutputTokens"], 131072);
    }

    #[test]
    fn test_adaptive_gemini3_low_effort() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Adaptive,
            effort: Some("low".to_string()),
            ..Default::default()
        });
        let body = json!({
            "model": "gemini-3-pro",
            "contents": [{"role": "user", "parts": [{"text": "Hi"}]}],
            "generationConfig": {
                "thinkingConfig": {
                    "includeThoughts": true,
                    "thinkingBudget": -1
                }
            }
        });
        let result = wrap_request(&body, "proj", "gemini-3-pro", None);
        let tc = &result["request"]["generationConfig"]["thinkingConfig"];
        assert_eq!(tc["thinkingLevel"], "LOW");
    }

    #[test]
    fn test_adaptive_gemini2_replaces_negative_budget() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Adaptive,
            ..Default::default()
        });
        let body = json!({
            "model": "gemini-2.5-pro",
            "contents": [{"role": "user", "parts": [{"text": "Hi"}]}],
            "generationConfig": {
                "thinkingConfig": {
                    "includeThoughts": true,
                    "thinkingBudget": -1
                }
            }
        });
        let result = wrap_request(&body, "proj", "gemini-2.5-pro", None);
        let tc = &result["request"]["generationConfig"]["thinkingConfig"];
        assert_eq!(tc["thinkingBudget"], 16000);
        assert!(tc.get("thinkingLevel").is_none() || tc["thinkingLevel"].is_null());
        assert_eq!(result["request"]["generationConfig"]["maxOutputTokens"], 131072);
    }

    #[test]
    fn test_non_adaptive_flash_cap_preserved() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Auto,
            ..Default::default()
        });
        let body = json!({
            "model": "gemini-3-flash",
            "contents": [{"role": "user", "parts": [{"text": "Hi"}]}],
            "generationConfig": {
                "thinkingConfig": {
                    "includeThoughts": true,
                    "thinkingBudget": 30000
                }
            }
        });
        let result = wrap_request(&body, "proj", "gemini-3-flash", None);
        let tc = &result["request"]["generationConfig"]["thinkingConfig"];
        assert_eq!(tc["thinkingBudget"], 24576);
    }

    #[test]
    fn test_adaptive_sets_max_output_131072() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        update_thinking_budget_config(ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Adaptive,
            ..Default::default()
        });
        let body = json!({
            "model": "gemini-3-pro",
            "contents": [{"role": "user", "parts": [{"text": "Hi"}]}],
            "generationConfig": {
                "thinkingConfig": {
                    "includeThoughts": true,
                    "thinkingBudget": -1
                },
                "maxOutputTokens": 48768
            }
        });
        let result = wrap_request(&body, "proj", "gemini-3-pro", None);
        assert_eq!(result["request"]["generationConfig"]["maxOutputTokens"], 131072);
    }
}
