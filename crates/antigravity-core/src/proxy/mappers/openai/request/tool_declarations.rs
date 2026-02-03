use serde_json::{json, Value};

pub fn transform_tool_declarations(tools: &[Value]) -> Vec<Value> {
    let mut function_declarations: Vec<Value> = Vec::new();

    for tool in tools.iter() {
        let mut gemini_func = if let Some(func) = tool.get("function") {
            func.clone()
        } else {
            let mut func = tool.clone();
            if let Some(obj) = func.as_object_mut() {
                obj.remove("type");
                obj.remove("strict");
                obj.remove("additionalProperties");
            }
            func
        };

        let name_opt = gemini_func
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(name) = &name_opt {
            if name == "web_search" || name == "google_search" || name == "web_search_20250305" {
                continue;
            }

            if name == "local_shell_call" {
                if let Some(obj) = gemini_func.as_object_mut() {
                    obj.insert("name".to_string(), json!("shell"));
                }
            }
        } else {
            tracing::warn!(
                "[OpenAI-Request] Skipping tool without name: {:?}",
                gemini_func
            );
            continue;
        }

        if let Some(obj) = gemini_func.as_object_mut() {
            obj.remove("format");
            obj.remove("strict");
            obj.remove("additionalProperties");
            obj.remove("type");
            obj.remove("external_web_access");
        }

        if let Some(params) = gemini_func.get_mut("parameters") {
            crate::proxy::common::json_schema::clean_json_schema(params);

            if let Some(params_obj) = params.as_object_mut() {
                if !params_obj.contains_key("type") {
                    params_obj.insert("type".to_string(), json!("OBJECT"));
                }
            }

            super::enforce_uppercase_types(params);
        } else {
            tracing::debug!(
                "[OpenAI-Request] Injecting default schema for custom tool: {}",
                gemini_func
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            );

            gemini_func.as_object_mut().unwrap().insert(
                "parameters".to_string(),
                json!({
                    "type": "OBJECT",
                    "properties": {
                        "content": {
                            "type": "STRING",
                            "description": "The raw content or patch to be applied"
                        }
                    },
                    "required": ["content"]
                }),
            );
        }
        function_declarations.push(gemini_func);
    }

    function_declarations
}
