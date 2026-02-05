//! Tool building for Gemini API.

use super::super::models::Tool;
use serde_json::{json, Value};

pub fn build_tools(
    tools: &Option<Vec<Tool>>,
    has_web_search: bool,
) -> Result<Option<Value>, String> {
    if let Some(tools_list) = tools {
        let mut function_declarations: Vec<Value> = Vec::new();
        let mut has_google_search = has_web_search;

        for tool in tools_list {
            // 1. Detect server tools / built-in tools like web_search
            if tool.is_web_search() {
                has_google_search = true;
                continue;
            }

            if let Some(t_type) = &tool.type_ {
                if t_type == "web_search_20250305" {
                    has_google_search = true;
                    continue;
                }
            }

            // 2. Detect by name
            if let Some(name) = &tool.name {
                if name == "web_search" || name == "google_search" {
                    has_google_search = true;
                    continue;
                }

                // 3. Client tools require input_schema
                let mut input_schema = tool.input_schema.clone().unwrap_or(json!({
                    "type": "object",
                    "properties": {}
                }));
                crate::proxy::common::json_schema::clean_json_schema(&mut input_schema);

                function_declarations.push(json!({
                    "name": name,
                    "description": tool.description,
                    "parameters": input_schema
                }));
            }
        }

        let mut tool_obj = serde_json::Map::new();

        // [repair] solve "Multiple tools are supported only when they are all search tools" 400 error
        // Principle: Gemini v1 internal interface is very picky, usually doesn't allow mixing Google Search and Function Declarations in the same tool definition.
        // For Claude CLI and other clients with MCP tools, must prioritize ensuring Function Declarations work properly.
        if !function_declarations.is_empty() {
            // If there are local tools, only use local tools, skip injecting Google Search
            tool_obj.insert("functionDeclarations".to_string(), json!(function_declarations));

            // [IMPROVED] Record reason for skipping googleSearch injection
            if has_google_search {
                tracing::info!(
                    "[Claude-Request] Skipping googleSearch injection due to {} existing function declarations. \
                     Gemini v1internal does not support mixed tool types.",
                    function_declarations.len()
                );
            }
        } else if has_google_search {
            // Only when there are no local tools, allow injecting Google Search
            tool_obj.insert("googleSearch".to_string(), json!({}));
        }

        if !tool_obj.is_empty() {
            return Ok(Some(json!([tool_obj])));
        }
    }

    Ok(None)
}
