// Gemini v1internal wrap/unwrap
use serde_json::{json, Value};

/// wraprequestbodyas v1internal format
pub fn wrap_request(
    body: &Value,
    project_id: &str,
    mapped_model: &str,
    session_id: Option<&str>,
) -> Value {
    // Priority: use passed mapped_model, otherwise attempt to get from body
    let original_model = body.get("model").and_then(|v| v.as_str()).unwrap_or(mapped_model);

    // If mapped_model is empty, use original_model
    let final_model_name = if !mapped_model.is_empty() { mapped_model } else { original_model };

    // Copy body for modification
    let mut inner_request = body.clone();

    // Deep cleanup [undefined] strings (commonly injected by Cherry Studio and other clients)
    crate::proxy::mappers::request_config::deep_clean_undefined(&mut inner_request);

    if let Some(contents) = inner_request.get_mut("contents") {
        crate::proxy::mappers::claude::request::image_retention::strip_old_images(contents);
    }

    // [FIX #765] Inject thought_signature into functionCall parts
    // Google requires thoughtSignature on ALL functionCall parts — use dummy as fallback
    if let Some(contents) = inner_request.get_mut("contents").and_then(|c| c.as_array_mut()) {
        let cached_sig = session_id
            .and_then(|s_id| crate::proxy::SignatureCache::global().get_session_signature(s_id));
        for content in contents {
            if let Some(parts) = content.get_mut("parts").and_then(|p| p.as_array_mut()) {
                for part in parts {
                    if part.get("functionCall").is_some() && part.get("thoughtSignature").is_none()
                    {
                        let sig = cached_sig.as_deref().unwrap_or(
                            crate::proxy::mappers::claude::request::signature_validator::DUMMY_SIGNATURE,
                        );
                        if let Some(obj) = part.as_object_mut() {
                            drop(obj.insert("thoughtSignature".to_string(), json!(sig)));
                        }
                    }
                }
            }
        }
    }

    // [FIX Issue #1355] Gemini Flash thinking budget capping
    // Force cap thinking_budget to 24576 for Flash models to prevent 400 Bad Request
    if final_model_name.to_lowercase().contains("flash") {
        if let Some(gen_config) = inner_request.get_mut("generationConfig") {
            if let Some(thinking_config) = gen_config.get_mut("thinkingConfig") {
                if let Some(budget_val) = thinking_config.get("thinkingBudget") {
                    if let Some(budget) = budget_val.as_u64() {
                        if budget > 24576 {
                            thinking_config["thinkingBudget"] = json!(24576);
                            tracing::info!(
                                "[Gemini-Wrap] Capped thinking_budget from {} to 24576 for model {}",
                                budget,
                                final_model_name
                            );
                        }
                    }
                }
            }
        }
    }

    // [FIX] Removed forced maxOutputTokens (64000) as it exceeds limits for Gemini 1.5 Flash/Pro standard models (8192).
    // This caused upstream to return empty/invalid responses, leading to 'NoneType' object has no attribute 'strip' in Python clients.
    // relying on upstream defaults or user provided values is safer.

    // Extract tools list for network detection (Gemini style may be nested)
    let tools_val: Option<Vec<Value>> =
        inner_request.get("tools").and_then(|t| t.as_array()).cloned();

    // Use shared grounding/config logic
    let config = crate::proxy::mappers::request_config::resolve_request_config(
        original_model,
        final_model_name,
        &tools_val,
        None,
        None,
    );

    // Clean tool declarations (remove forbidden Schema fields like multipleOf, and remove redundant search decls)
    if let Some(tools) = inner_request.get_mut("tools") {
        if let Some(tools_arr) = tools.as_array_mut() {
            for tool in tools_arr {
                if let Some(decls) = tool.get_mut("functionDeclarations") {
                    if let Some(decls_arr) = decls.as_array_mut() {
                        // 1. Filter out network keyword functions
                        decls_arr.retain(|decl| {
                            if let Some(name) = decl.get("name").and_then(|v| v.as_str()) {
                                if name == "web_search" || name == "google_search" {
                                    return false;
                                }
                            }
                            true
                        });

                        // 2. Clean remaining Schema
                        // [FIX] Gemini CLI uses parametersJsonSchema, while standard Gemini API uses parameters
                        // Need to rename parametersJsonSchema to parameters
                        for decl in decls_arr {
                            // detectandconvertfieldname
                            if let Some(decl_obj) = decl.as_object_mut() {
                                // If parametersJsonSchema exists, rename it to parameters
                                if let Some(params_json_schema) =
                                    decl_obj.remove("parametersJsonSchema")
                                {
                                    let mut params = params_json_schema;
                                    crate::proxy::common::json_schema::clean_json_schema(
                                        &mut params,
                                    );
                                    drop(decl_obj.insert("parameters".to_string(), params));
                                } else if let Some(params) = decl_obj.get_mut("parameters") {
                                    // standard parameters field
                                    crate::proxy::common::json_schema::clean_json_schema(params);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    tracing::debug!(
        "[Debug] Gemini Wrap: original='{}', mapped='{}', final='{}', type='{}'",
        original_model,
        final_model_name,
        config.final_model,
        config.request_type
    );

    // Inject googleSearch tool if needed
    if config.inject_google_search {
        crate::proxy::mappers::request_config::inject_google_search_tool(&mut inner_request);
    }

    // Inject imageConfig if present (for image generation models)
    if let Some(image_config) = config.image_config {
        if let Some(obj) = inner_request.as_object_mut() {
            // 1. Remove tools (image generation does not support tools)
            drop(obj.remove("tools"));

            // 2. Remove systemInstruction (image generation does not support system prompts)
            drop(obj.remove("systemInstruction"));

            // 3. Clean generationConfig (remove thinkingConfig, responseMimeType, responseModalities etc.)
            let gen_config = obj.entry("generationConfig").or_insert_with(|| json!({}));
            if let Some(gen_obj) = gen_config.as_object_mut() {
                drop(gen_obj.remove("thinkingConfig"));
                drop(gen_obj.remove("responseMimeType"));
                drop(gen_obj.remove("responseModalities")); // Cherry Studio sends this, might conflict
                drop(gen_obj.insert("imageConfig".to_string(), image_config));
            }
        }
    } else {
        // [NEW] Only inject Antigravity identity in non-image generation mode (simplified version)
        let antigravity_identity = "You are Antigravity, a powerful agentic AI coding assistant designed by the Google Deepmind team working on Advanced Agentic Coding.\n\
        You are pair programming with a USER to solve their coding task. The task may require creating a new codebase, modifying or debugging an existing codebase, or simply answering a question.\n\
        **Absolute paths only**\n\
        **Proactiveness**";

        // [HYBRID] checkwhetheralreadyhave systemInstruction
        if let Some(system_instruction) = inner_request.get_mut("systemInstruction") {
            // [NEW] complete role: user
            if let Some(obj) = system_instruction.as_object_mut() {
                if !obj.contains_key("role") {
                    drop(obj.insert("role".to_string(), json!("user")));
                }
            }

            if let Some(parts) = system_instruction.get_mut("parts") {
                if let Some(parts_array) = parts.as_array_mut() {
                    // Check if first part already contains Antigravity identity
                    let has_antigravity = parts_array
                        .first()
                        .and_then(|p| p.get("text"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.contains("You are Antigravity"))
                        .unwrap_or(false);

                    if !has_antigravity {
                        // Insert Antigravity identity at the beginning
                        parts_array.insert(0, json!({"text": antigravity_identity}));
                    }
                }
            }
        } else {
            // No systemInstruction, create a new one
            inner_request["systemInstruction"] = json!({
                "role": "user",
                "parts": [{"text": antigravity_identity}]
            });
        }
    }

    let final_request = json!({
        "project": project_id,
        "requestId": format!("agent-{}", uuid::Uuid::new_v4()), // Fixed with agent- prefix
        "request": inner_request,
        "model": config.final_model,
        "userAgent": "antigravity",
        "requestType": config.request_type
    });

    final_request
}

pub fn unwrap_response(response: &Value) -> Value {
    response.get("response").unwrap_or(response).clone()
}

#[cfg(test)]
#[path = "wrapper_tests.rs"]
mod wrapper_tests;
