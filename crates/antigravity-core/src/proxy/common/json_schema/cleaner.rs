//! JSON Schema cleaning entry points and utility functions.
//!
//! Functions for preparing JSON Schema for Gemini API compatibility.

use serde_json::Value;
use std::sync::LazyLock;

use super::super::tool_adapter::ToolAdapter;
use super::super::tool_adapters::PencilAdapter;
use super::recursive::clean_json_schema_recursive;

/// Static list of tool adapters for schema transformation.
static TOOL_ADAPTERS: LazyLock<Vec<Box<dyn ToolAdapter>>> =
    LazyLock::new(|| vec![Box::new(PencilAdapter)]);

/// Cleans JSON schema for a specific tool, applying tool-specific adapters.
pub fn clean_json_schema_for_tool(value: &mut Value, tool_name: &str) {
    let adapter = TOOL_ADAPTERS.iter().find(|a| a.matches(tool_name));

    if let Some(adapter) = adapter {
        let _ = adapter.pre_process(value);
    }

    clean_json_schema(value);

    if let Some(adapter) = adapter {
        let _ = adapter.post_process(value);
    }
}

/// Recursively cleans JSON Schema to conform to Gemini interface requirements.
///
/// 1. Expands $ref and $defs: replaces references with actual definitions
/// 2. Removes unsupported fields: $schema, additionalProperties, format, default, etc.
/// 3. Handles union types: `["string", "null"]` -> `"string"`
/// 4. Handles anyOf union types
/// 5. Converts type field values to lowercase (Gemini v1 internal requirement)
/// 6. Removes numeric validation fields: multipleOf, exclusiveMinimum, etc.
pub fn clean_json_schema(value: &mut Value) {
    let mut all_defs = serde_json::Map::new();
    collect_all_defs(value, &mut all_defs);

    if let Value::Object(map) = value {
        let _ = map.remove("$defs");
        let _ = map.remove("definitions");
    }

    if let Value::Object(map) = value {
        flatten_refs(map, &all_defs);
    }

    let _ = clean_json_schema_recursive(value);
}

/// Recursively collects all $defs and definitions from all nesting levels.
fn collect_all_defs(value: &Value, defs: &mut serde_json::Map<String, Value>) {
    if let Value::Object(map) = value {
        if let Some(Value::Object(d)) = map.get("$defs") {
            for (k, v) in d {
                let _ = defs.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        if let Some(Value::Object(d)) = map.get("definitions") {
            for (k, v) in d {
                let _ = defs.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        for (key, v) in map {
            if key != "$defs" && key != "definitions" {
                collect_all_defs(v, defs);
            }
        }
    } else if let Value::Array(arr) = value {
        for item in arr {
            collect_all_defs(item, defs);
        }
    }
}

/// Recursively expands $ref references.
fn flatten_refs(map: &mut serde_json::Map<String, Value>, defs: &serde_json::Map<String, Value>) {
    if let Some(Value::String(ref_path)) = map.remove("$ref") {
        let ref_name = ref_path.split('/').next_back().unwrap_or(&ref_path);

        if let Some(def_schema) = defs.get(ref_name) {
            if let Value::Object(def_map) = def_schema {
                for (k, v) in def_map {
                    let _ = map.entry(k.clone()).or_insert_with(|| v.clone());
                }
                flatten_refs(map, defs);
            }
        } else {
            let _ = map.insert("type".to_string(), serde_json::json!("string"));
            let hint = format!("(Unresolved $ref: {})", ref_path);
            let desc_val = map
                .entry("description".to_string())
                .or_insert_with(|| Value::String(String::new()));
            if let Value::String(s) = desc_val {
                if !s.contains(&hint) {
                    if !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(&hint);
                }
            }
        }
    }

    for (_, v) in map.iter_mut() {
        if let Value::Object(child_map) = v {
            flatten_refs(child_map, defs);
        } else if let Value::Array(arr) = v {
            for item in arr {
                if let Value::Object(item_map) = item {
                    flatten_refs(item_map, defs);
                }
            }
        }
    }
}
