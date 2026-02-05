//! Tool call argument remapping for Claude streaming responses.

use serde_json::{json, Value};

/// Coerces a JSON value to a boolean if possible.
#[allow(dead_code)]
fn coerce_to_bool(value: &Value) -> Option<Value> {
    match value {
        Value::Bool(_) => Some(value.clone()),
        Value::String(s) => {
            let lower = s.to_lowercase();
            if lower == "true" || lower == "yes" || lower == "1" || lower == "-n" {
                Some(json!(true))
            } else if lower == "false" || lower == "no" || lower == "0" {
                Some(json!(false))
            } else {
                None
            }
        },
        Value::Number(n) => Some(json!(n.as_i64().map(|i| i != 0).unwrap_or(false))),
        _ => None,
    }
}

/// Remaps function call arguments to match expected tool schemas.
///
/// Different AI models may produce slightly different argument names for the same
/// tool. This function normalizes common variations (e.g., `description` → `pattern`
/// for grep, `path` → `file_path` for read).
pub fn remap_function_call_args(name: &str, args: &mut Value) {
    if let Some(obj) = args.as_object() {
        tracing::debug!("[Streaming] Tool Call: '{}' Args: {:?}", name, obj);
    }

    if name == "EnterPlanMode" {
        if let Some(obj) = args.as_object_mut() {
            obj.clear();
        }
        return;
    }

    if let Some(obj) = args.as_object_mut() {
        match name.to_lowercase().as_str() {
            "grep" | "search" | "search_code_definitions" | "search_code_snippets" => {
                if let Some(desc) = obj.remove("description") {
                    if !obj.contains_key("pattern") {
                        drop(obj.insert("pattern".to_string(), desc));
                        tracing::debug!("[Streaming] Remapped Grep: description → pattern");
                    }
                }

                if let Some(query) = obj.remove("query") {
                    if !obj.contains_key("pattern") {
                        drop(obj.insert("pattern".to_string(), query));
                        tracing::debug!("[Streaming] Remapped Grep: query → pattern");
                    }
                }

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
                        tracing::debug!(
                            "[Streaming] Remapped Grep: paths → path(\"{}\")",
                            path_str
                        );
                    } else {
                        drop(obj.insert("path".to_string(), json!(".")));
                        tracing::debug!("[Streaming] Added default path: \".\"");
                    }
                }
            },
            "glob" => {
                if let Some(desc) = obj.remove("description") {
                    if !obj.contains_key("pattern") {
                        drop(obj.insert("pattern".to_string(), desc));
                        tracing::debug!("[Streaming] Remapped Glob: description → pattern");
                    }
                }

                if let Some(query) = obj.remove("query") {
                    if !obj.contains_key("pattern") {
                        drop(obj.insert("pattern".to_string(), query));
                        tracing::debug!("[Streaming] Remapped Glob: query → pattern");
                    }
                }

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
                        tracing::debug!(
                            "[Streaming] Remapped Glob: paths → path(\"{}\")",
                            path_str
                        );
                    } else {
                        drop(obj.insert("path".to_string(), json!(".")));
                        tracing::debug!("[Streaming] Added default path: \".\"");
                    }
                }
            },
            "read" => {
                if let Some(path) = obj.remove("path") {
                    if !obj.contains_key("file_path") {
                        drop(obj.insert("file_path".to_string(), path));
                        tracing::debug!("[Streaming] Remapped Read: path → file_path");
                    }
                }
            },
            "ls" => {
                if !obj.contains_key("path") {
                    drop(obj.insert("path".to_string(), json!(".")));
                    tracing::debug!("[Streaming] Remapped LS: default path → \".\"");
                }
            },
            other => {
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
                    drop(obj.insert("path".to_string(), json!(path)));
                    tracing::debug!(
                        "[Streaming] Probabilistic fix for tool '{}': paths[0] → path(\"{}\")",
                        other,
                        path
                    );
                }
                tracing::debug!(
                    "[Streaming] Unmapped tool call processed via generic rules: {} (keys: {:?})",
                    other,
                    obj.keys()
                );
            },
        }
    }
}
