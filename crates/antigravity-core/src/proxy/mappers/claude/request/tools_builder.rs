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

        // [修复] 解决 "Multiple tools are supported only when they are all search tools" 400 错误
        // 原理：Gemini v1internal 接口非常挑剔，通常不允许在同一个工具定义中混用 Google Search 和 Function Declarations。
        // 对于 Claude CLI 等携带 MCP 工具的客户端，必须优先保证 Function Declarations 正常工作。
        if !function_declarations.is_empty() {
            // 如果有本地工具，则只使用本地工具，放弃注入的 Google Search
            tool_obj.insert(
                "functionDeclarations".to_string(),
                json!(function_declarations),
            );

            // [IMPROVED] 记录跳过 googleSearch 注入的原因
            if has_google_search {
                tracing::info!(
                    "[Claude-Request] Skipping googleSearch injection due to {} existing function declarations. \
                     Gemini v1internal does not support mixed tool types.",
                    function_declarations.len()
                );
            }
        } else if has_google_search {
            // 只有在没有本地工具时，才允许注入 Google Search
            tool_obj.insert("googleSearch".to_string(), json!({}));
        }

        if !tool_obj.is_empty() {
            return Ok(Some(json!([tool_obj])));
        }
    }

    Ok(None)
}
