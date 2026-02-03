// Grounding and tool detection utilities
// Handles Google Search injection and networking tool detection

use serde_json::{json, Value};

/// Inject current googleSearch tool and ensure no duplicate legacy search tools
pub fn inject_google_search_tool(body: &mut Value) {
    if let Some(obj) = body.as_object_mut() {
        let tools_entry = obj.entry("tools").or_insert_with(|| json!([]));
        if let Some(tools_arr) = tools_entry.as_array_mut() {
            // [安全校验] 如果数组中已经包含 functionDeclarations，严禁注入 googleSearch
            // 因为 Gemini v1internal 不支持在一次请求中混用 search 和 functions
            let has_functions = tools_arr.iter().any(|t| {
                t.as_object()
                    .is_some_and(|o| o.contains_key("functionDeclarations"))
            });

            if has_functions {
                tracing::debug!(
                    "Skipping googleSearch injection due to existing functionDeclarations"
                );
                return;
            }

            // 首先清理掉已存在的 googleSearch 或 googleSearchRetrieval，以防重复产生冲突
            tools_arr.retain(|t| {
                if let Some(o) = t.as_object() {
                    !(o.contains_key("googleSearch") || o.contains_key("googleSearchRetrieval"))
                } else {
                    true
                }
            });

            // 注入统一的 googleSearch (v1internal 规范)
            tools_arr.push(json!({
                "googleSearch": {}
            }));
        }
    }
}

/// 深度迭代清理客户端发送的 [undefined] 脏字符串，防止 Gemini 接口校验失败
pub fn deep_clean_undefined(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // 移除值为 "[undefined]" 的键
            map.retain(|_, v| {
                if let Some(s) = v.as_str() {
                    s != "[undefined]"
                } else {
                    true
                }
            });
            // 递归处理嵌套
            for v in map.values_mut() {
                deep_clean_undefined(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                deep_clean_undefined(v);
            }
        }
        _ => {}
    }
}

/// Detects if the tool list contains a request for networking/web search.
/// Supported keywords: "web_search", "google_search", "web_search_20250305"
pub fn detects_networking_tool(tools: &Option<Vec<Value>>) -> bool {
    if let Some(list) = tools {
        for tool in list {
            // 1. 直发风格 (Claude/Simple OpenAI/Anthropic Builtin/Vertex): { "name": "..." } 或 { "type": "..." }
            if let Some(n) = tool.get("name").and_then(|v| v.as_str()) {
                if n == "web_search"
                    || n == "google_search"
                    || n == "web_search_20250305"
                    || n == "google_search_retrieval"
                {
                    return true;
                }
            }

            if let Some(t) = tool.get("type").and_then(|v| v.as_str()) {
                if t == "web_search_20250305"
                    || t == "google_search"
                    || t == "web_search"
                    || t == "google_search_retrieval"
                {
                    return true;
                }
            }

            // 2. OpenAI 嵌套风格: { "type": "function", "function": { "name": "..." } }
            if let Some(func) = tool.get("function") {
                if let Some(n) = func.get("name").and_then(|v| v.as_str()) {
                    let keywords = [
                        "web_search",
                        "google_search",
                        "web_search_20250305",
                        "google_search_retrieval",
                    ];
                    if keywords.contains(&n) {
                        return true;
                    }
                }
            }

            // 3. Gemini 原生风格: { "functionDeclarations": [ { "name": "..." } ] }
            if let Some(decls) = tool.get("functionDeclarations").and_then(|v| v.as_array()) {
                for decl in decls {
                    if let Some(n) = decl.get("name").and_then(|v| v.as_str()) {
                        if n == "web_search"
                            || n == "google_search"
                            || n == "google_search_retrieval"
                        {
                            return true;
                        }
                    }
                }
            }

            // 4. Gemini googleSearch 声明 (含 googleSearchRetrieval 变体)
            if tool.get("googleSearch").is_some() || tool.get("googleSearchRetrieval").is_some() {
                return true;
            }
        }
    }
    false
}

/// 探测是否包含非联网相关的本地函数工具
pub fn contains_non_networking_tool(tools: &Option<Vec<Value>>) -> bool {
    if let Some(list) = tools {
        for tool in list {
            let mut is_networking = false;

            // 简单逻辑：如果它是一个函数声明且名字不是联网关键词，则视为非联网工具
            if let Some(n) = tool.get("name").and_then(|v| v.as_str()) {
                let keywords = [
                    "web_search",
                    "google_search",
                    "web_search_20250305",
                    "google_search_retrieval",
                ];
                if keywords.contains(&n) {
                    is_networking = true;
                }
            } else if let Some(func) = tool.get("function") {
                if let Some(n) = func.get("name").and_then(|v| v.as_str()) {
                    let keywords = [
                        "web_search",
                        "google_search",
                        "web_search_20250305",
                        "google_search_retrieval",
                    ];
                    if keywords.contains(&n) {
                        is_networking = true;
                    }
                }
            } else if tool.get("googleSearch").is_some()
                || tool.get("googleSearchRetrieval").is_some()
            {
                is_networking = true;
            } else if tool.get("functionDeclarations").is_some() {
                // 如果是 Gemini 风格的 functionDeclarations，进去看一眼
                if let Some(decls) = tool.get("functionDeclarations").and_then(|v| v.as_array()) {
                    for decl in decls {
                        if let Some(n) = decl.get("name").and_then(|v| v.as_str()) {
                            let keywords =
                                ["web_search", "google_search", "google_search_retrieval"];
                            if !keywords.contains(&n) {
                                return true; // 发现本地函数
                            }
                        }
                    }
                }
                is_networking = true; // 即使全是联网，外层也标记为联网
            }

            if !is_networking {
                return true;
            }
        }
    }
    false
}
