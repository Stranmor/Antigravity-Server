//! JSON Schema cleaning entry points and utility functions
//!
//! Functions for preparing JSON Schema for Gemini API compatibility.

use once_cell::sync::Lazy;
use serde_json::Value;

use super::super::tool_adapter::ToolAdapter;
use super::super::tool_adapters::PencilAdapter;
use super::recursive::clean_json_schema_recursive;

static TOOL_ADAPTERS: Lazy<Vec<Box<dyn ToolAdapter>>> = Lazy::new(|| vec![Box::new(PencilAdapter)]);

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

/// recursivecleanup JSON Schema toconform to Gemini interfacerequirement
///
/// 1. [New] expand $ref  and  $defs: willreferencereplaceasactualdefinition，solve Gemini notsupport $ref  issue
/// 2. removeUnsupported field: $schema, additionalProperties, format, default, uniqueItems, validation fields
/// 3. handleuniontype: ["string", "null"] -> "string"
/// 4. [NEW] handle anyOf uniontype: anyOf: [{"type": "string"}, {"type": "null"}] -> "type": "string"
/// 5. will type field valueconvertaslowercase (Gemini v1internal requirement)
/// 6. removenumericvalidationfield: multipleOf, exclusiveMinimum, exclusiveMaximum etc
pub fn clean_json_schema(value: &mut Value) {
    // 0. pre-handle：expand $ref (Schema Flattening)
    // [FIX #952] recursivecollectalllevel  $defs/definitions，rather thanonlyfromrootlevelextract
    let mut all_defs = serde_json::Map::new();
    collect_all_defs(value, &mut all_defs);

    // removerootlevel  $defs/definitions (maintainbackwardcompatible)
    if let Value::Object(map) = value {
        map.remove("$defs");
        map.remove("definitions");
    }

    // [FIX #952] alwaysrun flatten_refs，even if defs is empty
    // this waycancaptureandhandlecannotparse  $ref (fallbackas string type)
    if let Value::Object(map) = value {
        flatten_refs(map, &all_defs);
    }

    // recursivecleanup
    clean_json_schema_recursive(value);
}

/// [NEW #952] recursivecollectalllevel  $defs  and  definitions
///
/// MCP tool  schema mayatanynestedleveldefinition $defs，rather thanonlyatrootlevel。
/// thisfunctiondepthtraverseentire schema，collectalldefinitiontounified  map in。
fn collect_all_defs(value: &Value, defs: &mut serde_json::Map<String, Value>) {
    if let Value::Object(map) = value {
        // collectcurrentlevel  $defs
        if let Some(Value::Object(d)) = map.get("$defs") {
            for (k, v) in d {
                // avoidoverwriteexisting definition（firstdefinition priority）
                defs.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        // collectcurrentlevel  definitions (Draft-07 style)
        if let Some(Value::Object(d)) = map.get("definitions") {
            for (k, v) in d {
                defs.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        // recursivehandleallchild nodes
        for (key, v) in map {
            // skip $defs/definitions itself，avoidduplicatehandle
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

/// recursiveexpand $ref
fn flatten_refs(map: &mut serde_json::Map<String, Value>, defs: &serde_json::Map<String, Value>) {
    // checkandreplace $ref
    if let Some(Value::String(ref_path)) = map.remove("$ref") {
        // parsereferencename (e.g. #/$defs/MyType -> MyType)
        let ref_name = ref_path.split('/').next_back().unwrap_or(&ref_path);

        if let Some(def_schema) = defs.get(ref_name) {
            // willdefinition contentmergetocurrent map
            if let Value::Object(def_map) = def_schema {
                for (k, v) in def_map {
                    // onlywhencurrent map does not havethe key whenonly theninsert (avoidoverwrite)
                    // butusually $ref nodeshould nothaveotherproperty
                    map.entry(k.clone()).or_insert_with(|| v.clone());
                }

                // recursivehandlejustmergein contentinmaycontaining  $ref
                // note：heremaywillinfiniterecursiveifexistcircularreference，buttooldefinitionusuallyis DAG
                flatten_refs(map, defs);
            }
        } else {
            // [FIX #952] cannotparse  $ref: convertasloose  string type，avoid API 400 error
            map.insert("type".to_string(), serde_json::json!("string"));
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

    // traversechild nodes
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
