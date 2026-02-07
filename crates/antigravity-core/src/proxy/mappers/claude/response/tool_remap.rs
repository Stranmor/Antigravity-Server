//! Tool argument remapping for Gemini → Claude compatibility.

use serde_json::json;

/// Remaps function call arguments for Gemini → Claude compatibility.
///
/// Gemini sometimes uses different parameter names than specified in tool schema.
pub(crate) fn remap_function_call_args(tool_name: &str, args: &mut serde_json::Value) {
    if let Some(obj) = args.as_object() {
        tracing::debug!("[Response] Tool Call: '{}' Args: {:?}", tool_name, obj);
    }

    if let Some(obj) = args.as_object_mut() {
        match tool_name.to_lowercase().as_str() {
            "grep" | "search" | "search_code_definitions" | "search_code_snippets" => {
                remap_grep_args(obj);
            },
            "glob" => {
                remap_glob_args(obj);
            },
            "read" => {
                remap_read_args(obj);
            },
            "ls" => {
                remap_ls_args(obj);
            },
            other => {
                remap_generic_args(other, obj);
            },
        }
    }
}

/// Remaps Grep tool arguments.
fn remap_grep_args(obj: &mut serde_json::Map<String, serde_json::Value>) {
    if let Some(desc) = obj.remove("description") {
        if !obj.contains_key("pattern") {
            drop(obj.insert("pattern".to_string(), desc));
            tracing::debug!("[Response] Remapped Grep: description → pattern");
        }
    }

    if let Some(query) = obj.remove("query") {
        if !obj.contains_key("pattern") {
            drop(obj.insert("pattern".to_string(), query));
            tracing::debug!("[Response] Remapped Grep: query → pattern");
        }
    }

    remap_paths_to_path(obj, "Grep");
}

/// Remaps Glob tool arguments.
fn remap_glob_args(obj: &mut serde_json::Map<String, serde_json::Value>) {
    if let Some(desc) = obj.remove("description") {
        if !obj.contains_key("pattern") {
            drop(obj.insert("pattern".to_string(), desc));
            tracing::debug!("[Response] Remapped Glob: description → pattern");
        }
    }

    if let Some(query) = obj.remove("query") {
        if !obj.contains_key("pattern") {
            drop(obj.insert("pattern".to_string(), query));
            tracing::debug!("[Response] Remapped Glob: query → pattern");
        }
    }

    remap_paths_to_path(obj, "Glob");
}

/// Remaps Read tool arguments.
fn remap_read_args(obj: &mut serde_json::Map<String, serde_json::Value>) {
    if let Some(path) = obj.remove("path") {
        if !obj.contains_key("file_path") {
            drop(obj.insert("file_path".to_string(), path));
            tracing::debug!("[Response] Remapped Read: path → file_path");
        }
    }
}

/// Remaps LS tool arguments.
fn remap_ls_args(obj: &mut serde_json::Map<String, serde_json::Value>) {
    if !obj.contains_key("path") {
        drop(obj.insert("path".to_string(), serde_json::json!(".")));
        tracing::debug!("[Response] Remapped LS: default path → \".\"");
    }
}

/// Generic argument remapping for unknown tools.
fn remap_generic_args(tool_name: &str, obj: &mut serde_json::Map<String, serde_json::Value>) {
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
        drop(obj.insert("path".to_string(), serde_json::json!(path)));
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

/// Converts "paths" array to "path" string.
fn remap_paths_to_path(obj: &mut serde_json::Map<String, serde_json::Value>, tool_name: &str) {
    if !obj.contains_key("path") {
        if let Some(paths) = obj.remove("paths") {
            let path_str = if let Some(arr) = paths.as_array() {
                arr.first().and_then(|v| v.as_str()).unwrap_or(".").to_string()
            } else if let Some(s) = paths.as_str() {
                s.to_string()
            } else {
                ".".to_string()
            };
            drop(obj.insert("path".to_string(), serde_json::json!(path_str)));
            tracing::debug!("[Response] Remapped {}: paths → path(\"{}\")", tool_name, path_str);
        } else {
            drop(obj.insert("path".to_string(), json!(".")));
            tracing::debug!("[Response] Added default path: \".\"");
        }
    }
}
