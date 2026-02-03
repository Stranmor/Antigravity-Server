// Grounding and tool detection utilities
// Handles Google Search injection and networking tool detection

use serde_json::{json, Value};

/// Inject current googleSearch tool and ensure no duplicate legacy search tools
pub fn inject_google_search_tool(body: &mut Value) {
    if let Some(obj) = body.as_object_mut() {
        let tools_entry = obj.entry("tools").or_insert_with(|| json!([]));
        if let Some(tools_arr) = tools_entry.as_array_mut() {
            // [Safety validation] If array already contains functionDeclarations, strictly prohibit injecting googleSearch
            // Because Gemini v1internal does not support mixing search and functions in one request
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

            // First cleanup existing googleSearch or googleSearchRetrieval to prevent duplicate conflicts
            tools_arr.retain(|t| {
                if let Some(o) = t.as_object() {
                    !(o.contains_key("googleSearch") || o.contains_key("googleSearchRetrieval"))
                } else {
                    true
                }
            });

            // Inject unified googleSearch (v1internal specification)
            tools_arr.push(json!({
                "googleSearch": {}
            }));
        }
    }
}

/// Deep iterative cleanup of client-sent [undefined] dirty strings to prevent Gemini API validation failure
pub fn deep_clean_undefined(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Remove keys with value "[undefined]"
            map.retain(|_, v| {
                if let Some(s) = v.as_str() {
                    s != "[undefined]"
                } else {
                    true
                }
            });
            // recursivehandlenested
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
            // 1. Direct style (Claude/Simple OpenAI/Anthropic Builtin/Vertex): { "name": "..." } or { "type": "..." }
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

            // 2. OpenAI nestedstyle: { "type": "function", "function": { "name": "..." } }
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

            // 3. Gemini native style: { "functionDeclarations": [ { "name": "..." } ] }
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

            // 4. Gemini googleSearch declaration (including googleSearchRetrieval variant)
            if tool.get("googleSearch").is_some() || tool.get("googleSearchRetrieval").is_some() {
                return true;
            }
        }
    }
    false
}

/// Detect whether containing non-networking related local function tools
pub fn contains_non_networking_tool(tools: &Option<Vec<Value>>) -> bool {
    if let Some(list) = tools {
        for tool in list {
            let mut is_networking = false;

            // Simple logic: if it is a function declaration and name is not a networking keyword, treat as non-networking tool
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
                // If it is Gemini style functionDeclarations, check inside
                if let Some(decls) = tool.get("functionDeclarations").and_then(|v| v.as_array()) {
                    for decl in decls {
                        if let Some(n) = decl.get("name").and_then(|v| v.as_str()) {
                            let keywords =
                                ["web_search", "google_search", "google_search_retrieval"];
                            if !keywords.contains(&n) {
                                return true; // Found local function
                            }
                        }
                    }
                }
                is_networking = true; // Even if all are networking, outer layer also marks as networking
            }

            if !is_networking {
                return true;
            }
        }
    }
    false
}
