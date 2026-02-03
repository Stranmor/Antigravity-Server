// Tool argument remapping for Gemini → Claude compatibility
// Extracted from response.rs

use serde_json::json;

/// [FIX #547] Helper function to coerce string values to boolean
/// Gemini sometimes sends boolean parameters as strings (e.g., "true", "-n", "false")
#[allow(dead_code)]
pub fn coerce_to_bool(value: &serde_json::Value) -> Option<serde_json::Value> {
    match value {
        serde_json::Value::Bool(_) => Some(value.clone()), // Already boolean
        serde_json::Value::String(s) => {
            let lower = s.to_lowercase();
            if lower == "true" || lower == "yes" || lower == "1" || lower == "-n" {
                Some(serde_json::json!(true))
            } else if lower == "false" || lower == "no" || lower == "0" {
                Some(serde_json::json!(false))
            } else {
                None // Unknown string, can't coerce
            }
        }
        serde_json::Value::Number(n) => Some(serde_json::json!(n
            .as_i64()
            .map(|i| i != 0)
            .unwrap_or(false))),
        _ => None,
    }
}

/// Known parameter remappings for Gemini → Claude compatibility
/// [FIX] Gemini sometimes uses different parameter names than specified in tool schema
pub fn remap_function_call_args(tool_name: &str, args: &mut serde_json::Value) {
    // [DEBUG] Always log incoming tool usage for diagnosis
    if let Some(obj) = args.as_object() {
        tracing::debug!("[Response] Tool Call: '{}' Args: {:?}", tool_name, obj);
    }

    if let Some(obj) = args.as_object_mut() {
        // [IMPROVED] Case-insensitive matching for tool names
        match tool_name.to_lowercase().as_str() {
            "grep" | "search" | "search_code_definitions" | "search_code_snippets" => {
                remap_grep_args(obj);
            }
            "glob" => {
                remap_glob_args(obj);
            }
            "read" => {
                remap_read_args(obj);
            }
            "ls" => {
                remap_ls_args(obj);
            }
            other => {
                remap_generic_args(other, obj);
            }
        }
    }
}

/// Remap Grep tool arguments
fn remap_grep_args(obj: &mut serde_json::Map<String, serde_json::Value>) {
    // [FIX] Gemini hallucination: maps parameter description to "description" field
    if let Some(desc) = obj.remove("description") {
        if !obj.contains_key("pattern") {
            obj.insert("pattern".to_string(), desc);
            tracing::debug!("[Response] Remapped Grep: description → pattern");
        }
    }

    // Gemini uses "query", Claude Code expects "pattern"
    if let Some(query) = obj.remove("query") {
        if !obj.contains_key("pattern") {
            obj.insert("pattern".to_string(), query);
            tracing::debug!("[Response] Remapped Grep: query → pattern");
        }
    }

    // [CRITICAL FIX] Claude Code uses "path" (string), NOT "paths" (array)!
    remap_paths_to_path(obj, "Grep");
}

/// Remap Glob tool arguments
fn remap_glob_args(obj: &mut serde_json::Map<String, serde_json::Value>) {
    // [FIX] Gemini hallucination: maps parameter description to "description" field
    if let Some(desc) = obj.remove("description") {
        if !obj.contains_key("pattern") {
            obj.insert("pattern".to_string(), desc);
            tracing::debug!("[Response] Remapped Glob: description → pattern");
        }
    }

    // Gemini uses "query", Claude Code expects "pattern"
    if let Some(query) = obj.remove("query") {
        if !obj.contains_key("pattern") {
            obj.insert("pattern".to_string(), query);
            tracing::debug!("[Response] Remapped Glob: query → pattern");
        }
    }

    // [CRITICAL FIX] Claude Code uses "path" (string), NOT "paths" (array)!
    remap_paths_to_path(obj, "Glob");
}

/// Remap Read tool arguments
fn remap_read_args(obj: &mut serde_json::Map<String, serde_json::Value>) {
    // Gemini might use "path" vs "file_path"
    if let Some(path) = obj.remove("path") {
        if !obj.contains_key("file_path") {
            obj.insert("file_path".to_string(), path);
            tracing::debug!("[Response] Remapped Read: path → file_path");
        }
    }
}

/// Remap LS tool arguments
fn remap_ls_args(obj: &mut serde_json::Map<String, serde_json::Value>) {
    // LS tool: ensure "path" parameter exists
    if !obj.contains_key("path") {
        obj.insert("path".to_string(), serde_json::json!("."));
        tracing::debug!("[Response] Remapped LS: default path → \".\"");
    }
}

/// Generic argument remapping for unknown tools
fn remap_generic_args(tool_name: &str, obj: &mut serde_json::Map<String, serde_json::Value>) {
    // [NEW] [Issue #785] Generic Property Mapping for all tools
    // If a tool has "paths" (array of 1) but no "path", convert it.
    let mut path_to_inject = None;
    if !obj.contains_key("path") {
        if let Some(paths) = obj.get("paths").and_then(|v| v.as_array()) {
            if paths.len() == 1 {
                if let Some(p) = paths[0].as_str() {
                    path_to_inject = Some(p.to_string());
                }
            }
        }
    }

    if let Some(path) = path_to_inject {
        obj.insert("path".to_string(), serde_json::json!(path));
        tracing::debug!(
            "[Response] Probabilistic fix for tool '{}': paths[0] → path(\"{}\")",
            tool_name,
            path
        );
    }
    tracing::debug!(
        "[Response] Unmapped tool call processed via generic rules: {} (keys: {:?})",
        tool_name,
        obj.keys()
    );
}

/// Helper: Convert "paths" array to "path" string
fn remap_paths_to_path(obj: &mut serde_json::Map<String, serde_json::Value>, tool_name: &str) {
    if !obj.contains_key("path") {
        if let Some(paths) = obj.remove("paths") {
            let path_str = if let Some(arr) = paths.as_array() {
                arr.first()
                    .and_then(|v| v.as_str())
                    .unwrap_or(".")
                    .to_string()
            } else if let Some(s) = paths.as_str() {
                s.to_string()
            } else {
                ".".to_string()
            };
            obj.insert("path".to_string(), serde_json::json!(path_str));
            tracing::debug!(
                "[Response] Remapped {}: paths → path(\"{}\")",
                tool_name,
                path_str
            );
        } else {
            // Default to current directory if missing
            obj.insert("path".to_string(), json!("."));
            tracing::debug!("[Response] Added default path: \".\"");
        }
    }
}
