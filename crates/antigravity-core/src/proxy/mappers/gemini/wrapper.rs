// Gemini v1internal 包装/解包
use serde_json::{json, Value};

/// 包装请求体为 v1internal 格式
pub fn wrap_request(
    body: &Value,
    project_id: &str,
    mapped_model: &str,
    session_id: Option<&str>,
) -> Value {
    // 优先使用传入的 mapped_model，其次尝试从 body 获取
    let original_model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(mapped_model);

    // 如果 mapped_model 是空的，则使用 original_model
    let final_model_name = if !mapped_model.is_empty() {
        mapped_model
    } else {
        original_model
    };

    // 复制 body 以便修改
    let mut inner_request = body.clone();

    // 深度清理 [undefined] 字符串 (Cherry Studio 等客户端常见注入)
    crate::proxy::mappers::request_config::deep_clean_undefined(&mut inner_request);

    // [FIX #765] Inject thought_signature into functionCall parts
    if let Some(s_id) = session_id {
        if let Some(contents) = inner_request
            .get_mut("contents")
            .and_then(|c| c.as_array_mut())
        {
            for content in contents {
                if let Some(parts) = content.get_mut("parts").and_then(|p| p.as_array_mut()) {
                    for part in parts {
                        if part.get("functionCall").is_some() {
                            // Only inject if it doesn't already have one
                            if part.get("thoughtSignature").is_none() {
                                if let Some(sig) = crate::proxy::SignatureCache::global()
                                    .get_session_signature(s_id)
                                {
                                    if let Some(obj) = part.as_object_mut() {
                                        obj.insert("thoughtSignature".to_string(), json!(sig));
                                        tracing::debug!(
                                            "[Gemini-Wrap] Injected signature (len: {}) for session: {}",
                                            sig.len(),
                                            s_id
                                        );
                                    }
                                }
                            }
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

    // 提取 tools 列表以进行联网探测 (Gemini 风格可能是嵌套的)
    let tools_val: Option<Vec<Value>> = inner_request
        .get("tools")
        .and_then(|t| t.as_array())
        .cloned();

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
                        // 1. 过滤掉联网关键字函数
                        decls_arr.retain(|decl| {
                            if let Some(name) = decl.get("name").and_then(|v| v.as_str()) {
                                if name == "web_search" || name == "google_search" {
                                    return false;
                                }
                            }
                            true
                        });

                        // 2. 清洗剩余 Schema
                        // [FIX] Gemini CLI 使用 parametersJsonSchema，而标准 Gemini API 使用 parameters
                        // 需要将 parametersJsonSchema 重命名为 parameters
                        for decl in decls_arr {
                            // 检测并转换字段名
                            if let Some(decl_obj) = decl.as_object_mut() {
                                // 如果存在 parametersJsonSchema，将其重命名为 parameters
                                if let Some(params_json_schema) =
                                    decl_obj.remove("parametersJsonSchema")
                                {
                                    let mut params = params_json_schema;
                                    crate::proxy::common::json_schema::clean_json_schema(
                                        &mut params,
                                    );
                                    decl_obj.insert("parameters".to_string(), params);
                                } else if let Some(params) = decl_obj.get_mut("parameters") {
                                    // 标准 parameters 字段
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
            obj.remove("tools");

            // 2. Remove systemInstruction (image generation does not support system prompts)
            obj.remove("systemInstruction");

            // 3. Clean generationConfig (remove thinkingConfig, responseMimeType, responseModalities etc.)
            let gen_config = obj.entry("generationConfig").or_insert_with(|| json!({}));
            if let Some(gen_obj) = gen_config.as_object_mut() {
                gen_obj.remove("thinkingConfig");
                gen_obj.remove("responseMimeType");
                gen_obj.remove("responseModalities"); // Cherry Studio sends this, might conflict
                gen_obj.insert("imageConfig".to_string(), image_config);
            }
        }
    } else {
        // [NEW] 只在非图像生成模式下注入 Antigravity 身份 (原始简化版)
        let antigravity_identity = "You are Antigravity, a powerful agentic AI coding assistant designed by the Google Deepmind team working on Advanced Agentic Coding.\n\
        You are pair programming with a USER to solve their coding task. The task may require creating a new codebase, modifying or debugging an existing codebase, or simply answering a question.\n\
        **Absolute paths only**\n\
        **Proactiveness**";

        // [HYBRID] 检查是否已有 systemInstruction
        if let Some(system_instruction) = inner_request.get_mut("systemInstruction") {
            // [NEW] 补全 role: user
            if let Some(obj) = system_instruction.as_object_mut() {
                if !obj.contains_key("role") {
                    obj.insert("role".to_string(), json!("user"));
                }
            }

            if let Some(parts) = system_instruction.get_mut("parts") {
                if let Some(parts_array) = parts.as_array_mut() {
                    // 检查第一个 part 是否已包含 Antigravity 身份
                    let has_antigravity = parts_array
                        .first()
                        .and_then(|p| p.get("text"))
                        .and_then(|t| t.as_str())
                        .map(|s| s.contains("You are Antigravity"))
                        .unwrap_or(false);

                    if !has_antigravity {
                        // 在前面插入 Antigravity 身份
                        parts_array.insert(0, json!({"text": antigravity_identity}));
                    }
                }
            }
        } else {
            // 没有 systemInstruction,创建一个新的
            inner_request["systemInstruction"] = json!({
                "role": "user",
                "parts": [{"text": antigravity_identity}]
            });
        }
    }

    let final_request = json!({
        "project": project_id,
        "requestId": format!("agent-{}", uuid::Uuid::new_v4()), // 修正为 agent- 前缀
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
