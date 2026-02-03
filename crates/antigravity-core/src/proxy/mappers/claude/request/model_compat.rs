//! Model compatibility checking.

use serde_json::Value;

pub fn clean_thinking_fields_recursive(val: &mut Value) {
    match val {
        Value::Object(map) => {
            map.remove("thought");
            map.remove("thoughtSignature");
            for (_, v) in map.iter_mut() {
                clean_thinking_fields_recursive(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                clean_thinking_fields_recursive(v);
            }
        }
        _ => {}
    }
}

/// Check if two model strings are compatible (same family)
pub fn is_model_compatible(cached: &str, target: &str) -> bool {
    // Simple heuristic: check if they share the same base prefix
    // e.g. "gemini-1.5-pro" vs "gemini-1.5-pro-002" -> Compatible
    // "gemini-1.5-pro" vs "gemini-2.0-flash" -> Incompatible

    // Normalize
    let c = cached.to_lowercase();
    let t = target.to_lowercase();

    if c == t {
        return true;
    }

    // Check specific families
    // Vertex AI signatures are very strict. 1.5-pro vs 1.5-flash are NOT cross-compatible.
    // 2.0-flash vs 2.0-pro are also NOT cross-compatible.

    // Exact model string match (already handled by c == t)

    // Grouped family match (Claude models are more permissive)
    if c.contains("claude-3-5") && t.contains("claude-3-5") {
        return true;
    }
    if c.contains("claude-3-7") && t.contains("claude-3-7") {
        return true;
    }

    // Gemini models: strict family match required for signatures
    if c.contains("gemini-1.5-pro") && t.contains("gemini-1.5-pro") {
        return true;
    }
    if c.contains("gemini-1.5-flash") && t.contains("gemini-1.5-flash") {
        return true;
    }
    if c.contains("gemini-2.0-flash") && t.contains("gemini-2.0-flash") {
        return true;
    }
    if c.contains("gemini-2.0-pro") && t.contains("gemini-2.0-pro") {
        return true;
    }

    // Fallback: strict match required
    false
}
