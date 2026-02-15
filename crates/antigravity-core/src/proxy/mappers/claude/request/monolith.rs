use super::super::models::*;
use super::generation_config::build_generation_config;
use super::google_content::build_google_contents;
use super::message_cleaning::{
    clean_cache_control_from_messages, deep_clean_cache_control, merge_consecutive_messages,
    sort_thinking_blocks_first,
};
use super::safety::build_safety_settings;
use super::system_instruction::build_system_instruction;
use super::thinking::{has_valid_signature_for_function_calls, should_enable_thinking_by_default};
use super::tools_builder::build_tools;
use crate::proxy::session_manager::SessionManager;
use crate::proxy::SignatureCache;
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn transform_claude_request_in(
    claude_req: &ClaudeRequest,
    project_id: &str,
    is_retry: bool,
) -> Result<Value, String> {
    let mut cleaned_req = claude_req.clone();
    merge_consecutive_messages(&mut cleaned_req.messages);
    clean_cache_control_from_messages(&mut cleaned_req.messages);
    sort_thinking_blocks_first(&mut cleaned_req.messages);

    let claude_req = &cleaned_req;

    let session_id = SessionManager::extract_session_id(claude_req);
    tracing::debug!("[Claude-Request] Session ID: {}", session_id);

    let has_web_search_tool = claude_req
        .tools
        .as_ref()
        .map(|tools| {
            tools.iter().any(|t| {
                t.is_web_search()
                    || t.name.as_deref() == Some("google_search")
                    || t.type_.as_deref() == Some("web_search_20250305")
            })
        })
        .unwrap_or(false);

    let mut tool_id_to_name: HashMap<String, String> = HashMap::new();
    let mut tool_name_to_schema: HashMap<String, Value> = HashMap::new();
    if let Some(tools) = &claude_req.tools {
        for tool in tools {
            if let (Some(name), Some(schema)) = (&tool.name, &tool.input_schema) {
                tool_name_to_schema.insert(name.clone(), schema.clone());
            }
        }
    }

    let has_mcp_tools = claude_req
        .tools
        .as_ref()
        .map(|tools| {
            tools.iter().any(|t| t.name.as_deref().map(|n| n.starts_with("mcp__")).unwrap_or(false))
        })
        .unwrap_or(false);

    let system_instruction =
        build_system_instruction(&claude_req.system, &claude_req.model, has_mcp_tools);

    const WEB_SEARCH_FALLBACK_MODEL: &str = "gemini-2.5-flash";
    let mapped_model = if has_web_search_tool {
        tracing::debug!(
            "[Claude-Request] Web search tool detected, using fallback model: {}",
            WEB_SEARCH_FALLBACK_MODEL
        );
        WEB_SEARCH_FALLBACK_MODEL.to_string()
    } else {
        crate::proxy::common::model_mapping::map_claude_model_to_gemini(&claude_req.model)
            .ok_or_else(|| format!("Unknown model: {}", claude_req.model))?
    };

    let tools_val: Option<Vec<Value>> = claude_req
        .tools
        .as_ref()
        .map(|list| list.iter().map(|t| serde_json::to_value(t).unwrap_or(json!({}))).collect());

    let config = crate::proxy::mappers::request_config::resolve_request_config(
        &claude_req.model,
        &mapped_model,
        &tools_val,
        None,
        None,
    );

    // [CRITICAL FIX] Disable dummy thought injection for Vertex AI
    // Vertex AI rejects thinking blocks without valid signatures
    // Even if thinking is enabled, we should NOT inject dummy blocks for historical messages
    let allow_dummy_thought = false;

    // Check if thinking is enabled in the request
    let mut is_thinking_enabled =
        claude_req.thinking.as_ref().map(|t| t.type_ == "enabled").unwrap_or_else(|| {
            // [Claude Code v2.0.67+] Default thinking enabled for Opus 4.5
            // If no thinking config is provided, enable by default for Opus models
            should_enable_thinking_by_default(&claude_req.model)
        });

    // [FIX] Check if target model supports thinking.
    // Gemini 2.5+, Gemini 3.x, and Claude models all support thinking natively via thinkingConfig.
    // Only legacy Gemini models (1.5-*, 2.0-*) and non-gemini/non-claude models lack support.
    let model_lower = mapped_model.to_lowercase();
    let target_model_supports_thinking = if model_lower.starts_with("claude-") {
        true
    } else if model_lower.starts_with("gemini-") {
        // Legacy models that don't support thinking
        let is_legacy =
            model_lower.starts_with("gemini-1.") || model_lower.starts_with("gemini-2.0");
        !is_legacy
    } else {
        // Unknown provider — assume no thinking support
        false
    };

    if is_thinking_enabled && !target_model_supports_thinking {
        tracing::warn!(
            "[Thinking-Mode] Target model '{}' does not support thinking. Force disabling thinking mode.",
            mapped_model
        );
        is_thinking_enabled = false;
    }

    // [2026-02-07] Removed should_disable_thinking_due_to_history — it caused infinite
    // degradation loops. Upstream API handles mixed ToolUse/Thinking history natively.
    // For thinking models, thinking MUST remain enabled to preserve hidden state quality.
    // [FIX #295 & #298] Strict signature enforcement for thinking models.
    // Instead of silently degrading (which causes quality loss and infinite loops),
    // we return an error when signatures are missing so the client knows.
    if is_thinking_enabled {
        let global_sig = SignatureCache::global().get_session_signature(&session_id);

        // Check if there are any thinking blocks in message history
        let has_thinking_history = claude_req.messages.iter().any(|m| {
            if m.role == "assistant" {
                if let MessageContent::Array(blocks) = &m.content {
                    return blocks.iter().any(|b| matches!(b, ContentBlock::Thinking { .. }));
                }
            }
            false
        });

        // Check if there are function calls in the request
        let has_function_calls = claude_req.messages.iter().any(|m| {
            if let MessageContent::Array(blocks) = &m.content {
                blocks.iter().any(|b| matches!(b, ContentBlock::ToolUse { .. }))
            } else {
                false
            }
        });

        if !has_thinking_history {
            tracing::info!(
                "[Thinking-Mode] First thinking request detected. Using permissive mode - \
                 signature validation will be handled by upstream API."
            );
        }

        // Only enforce signature checks when we have both function calls AND thinking history
        // (first-time requests don't need signatures yet)
        let needs_signature_check = has_function_calls && has_thinking_history;

        if needs_signature_check
            && !has_valid_signature_for_function_calls(
                &claude_req.messages,
                &global_sig,
                &session_id,
            )
        {
            tracing::error!(
                "[Thinking-Mode] CRITICAL: No valid signature found for function calls \
                 in thinking mode. Hidden state integrity cannot be guaranteed. \
                 Session: {}, Model: {}",
                session_id,
                claude_req.model
            );
            return Err(format!(
                "Thinking mode requires valid signatures for function calls but none were found. \
                 Session: {}. This indicates lost hidden state — response quality cannot be guaranteed. \
                 Please start a new conversation.",
                session_id
            ));
        }
    }

    // 4. Generation Config & Thinking (Pass final is_thinking_enabled)
    let generation_config = build_generation_config(
        claude_req,
        has_web_search_tool,
        is_thinking_enabled,
        &mapped_model,
    );

    // 2. Contents (Messages)
    let mut contents = build_google_contents(
        &claude_req.messages,
        claude_req,
        &mut tool_id_to_name,
        is_thinking_enabled,
        allow_dummy_thought,
        &mapped_model,
        &session_id,
        is_retry,
        &tool_name_to_schema,
    )?;

    // Strip images from old user messages to prevent token accumulation
    crate::proxy::common::image_retention::strip_old_images(&mut contents);

    // 3. Tools
    let tools = build_tools(&claude_req.tools, has_web_search_tool)?;

    // 5. Safety Settings (configurable via GEMINI_SAFETY_THRESHOLD env var)
    let safety_settings = build_safety_settings();

    // Build inner request
    let mut inner_request = json!({
        "contents": contents,
        "safetySettings": safety_settings,
    });

    // Deep cleanup of [undefined] strings (commonly injected by Cherry Studio and other clients)
    crate::proxy::mappers::request_config::deep_clean_undefined(&mut inner_request);

    if let Some(sys_inst) = system_instruction {
        inner_request["systemInstruction"] = sys_inst;
    }

    if !generation_config.is_null() {
        inner_request["generationConfig"] = generation_config;
    }

    if let Some(tools_val) = tools {
        inner_request["tools"] = tools_val;
        // Explicitly set tool config mode as VALIDATED
        inner_request["toolConfig"] = json!({
            "functionCallingConfig": {
                "mode": "VALIDATED"
            }
        });
    }

    // Inject googleSearch tool if needed (and not already done by build_tools)
    if config.inject_google_search && !has_web_search_tool {
        crate::proxy::mappers::request_config::inject_google_search_tool(&mut inner_request);
    }

    // Inject imageConfig if present (for image generation models)
    if let Some(image_config) = config.image_config {
        if let Some(obj) = inner_request.as_object_mut() {
            // 1. Remove tools (image generation does not support tools)
            obj.remove("tools");

            // 2. Remove systemInstruction (image generation does not support system prompts)
            obj.remove("systemInstruction");

            // 3. Clean generationConfig (remove thinkingConfig, responseMimeType, responseModalities etc.)
            let gen_config = obj.entry("generationConfig").or_insert_with(|| json!({}));
            if let Some(gen_obj) = gen_config.as_object_mut() {
                gen_obj.remove("thinkingConfig");
                gen_obj.remove("responseMimeType");
                gen_obj.remove("responseModalities");
                gen_obj.insert("imageConfig".to_string(), image_config);
            }
        }
    }

    // generate requestId
    let request_id = format!("agent-{}", uuid::Uuid::new_v4());

    // Build final request body
    let mut body = json!({
        "project": project_id,
        "requestId": request_id,
        "request": inner_request,
        "model": config.final_model,
        "userAgent": "antigravity",
        "requestType": config.request_type,
    });

    // If metadata.user_id is provided, reuse it as sessionId
    if let Some(metadata) = &claude_req.metadata {
        if let Some(user_id) = &metadata.user_id {
            body["request"]["sessionId"] = json!(user_id);
        }
    }

    // [FIX #593] Recursively deep-clean all cache_control fields
    deep_clean_cache_control(&mut body);

    // Strip Gemini-only dummy signatures for Claude models on Vertex AI
    if mapped_model.starts_with("claude-") {
        tracing::info!(
            "[Claude-Vertex] Stripping non-Claude signatures for model: {}",
            mapped_model
        );
        super::signature_stripping::strip_non_claude_thought_signatures(&mut body);
        super::signature_stripping::repair_tool_pairing_after_strip(&mut body);
    }

    Ok(body)
}
