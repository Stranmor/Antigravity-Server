mod content_parts;
mod generation_config;
mod message_transform;
mod tool_declarations;

#[cfg(test)]
mod tests;

use super::models::*;
use crate::proxy::session_manager::SessionManager;
use crate::proxy::SignatureCache;
use generation_config::build_generation_config;
use message_transform::{merge_consecutive_roles, transform_message, MessageTransformContext};
use serde_json::{json, Value};

pub fn transform_openai_request(
    request: &OpenAIRequest,
    project_id: &str,
    mapped_model: &str,
) -> Value {
    // Convert OpenAI tools to Value array for detection
    let tools_val = request.tools.as_ref().map(|list| list.to_vec());

    let mapped_model_lower = mapped_model.to_lowercase();

    // Resolve grounding config
    let config = crate::proxy::mappers::request_config::resolve_request_config(
        &request.model,
        &mapped_model_lower,
        &tools_val,
        request.size.as_deref(),
        request.quality.as_deref(),
    );

    // [FIX] Detect thinking models early (needed for signature handling)
    // Only treat as Gemini thinking if model name explicitly contains "-thinking"
    // Avoid injecting thinkingConfig for gemini-3-pro (preview) which doesn't support it
    let is_gemini_3_thinking = mapped_model_lower.contains("gemini")
        && mapped_model_lower.contains("-thinking")
        && !mapped_model_lower.contains("claude");
    let is_claude_thinking = mapped_model_lower.ends_with("-thinking");
    let is_thinking_model = is_gemini_3_thinking || is_claude_thinking;

    // [NEW] Check if history messages are compatible with thinking model (whether Assistant messages are missing reasoning_content)
    let has_incompatible_assistant_history = request.messages.iter().any(|msg| {
        msg.role == "assistant"
            && msg.reasoning_content.as_ref().map(|s| s.is_empty()).unwrap_or(true)
    });

    // [FIX #signature-recovery] Generate session_id and get signature from SignatureCache (not legacy global store)
    let session_id = SessionManager::extract_openai_session_id(request);
    let session_thought_sig = SignatureCache::global().get_session_signature(&session_id);

    if let Some(ref sig) = session_thought_sig {
        tracing::debug!(
            "[OpenAI-Thinking] Recovered signature from session cache (session: {}, len: {})",
            session_id,
            sig.len()
        );
    }

    // [NEW] Decide whether to enable Thinking feature:
    // If it's a Claude thinking model and history is incompatible and no available signature to use as placeholder, disable Thinking to prevent 400
    let actual_include_thinking = if is_claude_thinking
        && has_incompatible_assistant_history
        && session_thought_sig.is_none()
    {
        tracing::warn!("[OpenAI-Thinking] Incompatible assistant history detected for Claude thinking model without session signature. Disabling thinking for this request to avoid 400 error.");
        false
    } else {
        is_thinking_model
    };

    tracing::debug!(
        "[Debug] OpenAI Request: original='{}', mapped='{}', type='{}', has_image_config={}",
        request.model,
        mapped_model,
        config.request_type,
        config.image_config.is_some()
    );

    // 1. Extract all System Messages and inject patches
    let mut system_instructions: Vec<String> = request
        .messages
        .iter()
        .filter(|msg| msg.role == "system" || msg.role == "developer")
        .filter_map(|msg| {
            msg.content.as_ref().map(|c| match c {
                OpenAIContent::String(s) => s.clone(),
                OpenAIContent::Array(blocks) => blocks
                    .iter()
                    .filter_map(|b| {
                        if let OpenAIContentBlock::Text { text } = b {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            })
        })
        .collect();

    // [NEW] ifrequestincontaining instructions field，priorityuseit
    if let Some(inst) = &request.instructions {
        if !inst.is_empty() {
            system_instructions.insert(0, inst.clone());
        }
    }

    // Pre-scan to map tool_call_id to function name (for Codex)
    let mut tool_id_to_name = std::collections::HashMap::new();
    for msg in &request.messages {
        if let Some(tool_calls) = &msg.tool_calls {
            for call in tool_calls {
                let name = &call.function.name;
                let final_name = if name == "local_shell_call" { "shell" } else { name };
                drop(tool_id_to_name.insert(call.id.clone(), final_name.to_string()));
            }
        }
    }

    // [NEW] Build tool_name → schema mapping for argument type correction (upstream v4.0.5)
    let mut tool_name_to_schema = std::collections::HashMap::new();
    if let Some(tools) = &request.tools {
        for tool in tools {
            if let (Some(name), Some(params)) = (
                tool.get("function").and_then(|f| f.get("name")).and_then(|v| v.as_str()),
                tool.get("function").and_then(|f| f.get("parameters")),
            ) {
                drop(tool_name_to_schema.insert(name.to_string(), params.clone()));
            } else if let (Some(name), Some(params)) =
                (tool.get("name").and_then(|v| v.as_str()), tool.get("parameters"))
            {
                // Handle simplified format some clients may use
                drop(tool_name_to_schema.insert(name.to_string(), params.clone()));
            }
        }
    }

    // Get thoughtSignature from session cache (PR #93 support)
    // (Already fetched as session_thought_sig above)
    if let Some(ref sig) = session_thought_sig {
        tracing::debug!("Got thoughtSignature from session cache (length: {})", sig.len());
    }

    let transform_ctx = MessageTransformContext {
        global_thought_sig: &session_thought_sig,
        actual_include_thinking,
        is_thinking_model,
        mapped_model,
        tool_id_to_name: &tool_id_to_name,
        tool_name_to_schema: &tool_name_to_schema,
    };

    let contents: Vec<Value> = request
        .messages
        .iter()
        .filter(|msg| msg.role != "system" && msg.role != "developer")
        .map(|msg| transform_message(msg, &transform_ctx))
        .filter(|msg| !msg["parts"].as_array().map(|a| a.is_empty()).unwrap_or(true))
        .collect();

    let contents = merge_consecutive_roles(contents);

    let gen_config = build_generation_config(request, actual_include_thinking, mapped_model);

    let mut inner_request = json!({
        "contents": contents,
        "generationConfig": gen_config,
        "safetySettings": [
            { "category": "HARM_CATEGORY_HARASSMENT", "threshold": "OFF" },
            { "category": "HARM_CATEGORY_HATE_SPEECH", "threshold": "OFF" },
            { "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT", "threshold": "OFF" },
            { "category": "HARM_CATEGORY_DANGEROUS_CONTENT", "threshold": "OFF" },
        ]
    });

    // Deep cleanup [undefined] strings (commonly injected by Cherry Studio and other clients)
    crate::proxy::mappers::request_config::deep_clean_undefined(&mut inner_request);

    // 4. Handle Tools
    if let Some(tools) = &request.tools {
        let function_declarations = tool_declarations::transform_tool_declarations(tools);
        if !function_declarations.is_empty() {
            inner_request["tools"] = json!([{ "functionDeclarations": function_declarations }]);
        }
    }

    // [NEW] Antigravity identity instruction (simplified version)
    let antigravity_identity = "You are Antigravity, a powerful agentic AI coding assistant designed by the Google Deepmind team working on Advanced Agentic Coding.\n\
    You are pair programming with a USER to solve their coding task. The task may require creating a new codebase, modifying or debugging an existing codebase, or simply answering a question.\n\
    **Absolute paths only**\n\
    **Proactiveness**";

    // [HYBRID] Check if user already provided Antigravity identity
    let user_has_antigravity =
        system_instructions.iter().any(|s| s.contains("You are Antigravity"));

    let mut parts = Vec::new();

    // 1. Antigravity identity (if needed, insert as separate Part)
    if !user_has_antigravity {
        parts.push(json!({"text": antigravity_identity}));
    }

    // 2. Append user instructions (as separate Parts)
    for inst in system_instructions {
        parts.push(json!({"text": inst}));
    }

    inner_request["systemInstruction"] = json!({
        "role": "user",
        "parts": parts
    });

    if config.inject_google_search {
        crate::proxy::mappers::request_config::inject_google_search_tool(&mut inner_request);
    }

    if let Some(image_config) = config.image_config {
        if let Some(obj) = inner_request.as_object_mut() {
            drop(obj.remove("tools"));
            drop(obj.remove("systemInstruction"));
            let gen_config = obj.entry("generationConfig").or_insert_with(|| json!({}));
            if let Some(gen_obj) = gen_config.as_object_mut() {
                drop(gen_obj.remove("thinkingConfig"));
                drop(gen_obj.remove("responseMimeType"));
                drop(gen_obj.remove("responseModalities"));
                drop(gen_obj.insert("imageConfig".to_string(), image_config));
            }
        }
    }

    json!({
        "project": project_id,
        "requestId": format!("openai-{}", uuid::Uuid::new_v4()),
        "request": inner_request,
        "model": config.final_model,
        "userAgent": "antigravity",
        "requestType": config.request_type
    })
}

fn enforce_uppercase_types(value: &mut Value) {
    if let Value::Object(map) = value {
        if let Some(Value::String(ref mut s)) = map.get_mut("type") {
            *s = s.to_uppercase();
        }
        if let Some(Value::Object(ref mut props)) = map.get_mut("properties") {
            for v in props.values_mut() {
                enforce_uppercase_types(v);
            }
        }
        if let Some(items) = map.get_mut("items") {
            enforce_uppercase_types(items);
        }
    } else if let Value::Array(arr) = value {
        for item in arr {
            enforce_uppercase_types(item);
        }
    }
}
