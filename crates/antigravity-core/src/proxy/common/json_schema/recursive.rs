//! Recursive JSON schema cleaning and normalization.

use serde_json::Value;
use std::collections::HashSet;

use super::merge::merge_all_of;
use super::union::extract_best_schema_from_union;

/// Recursively cleans a JSON schema, normalizing types and removing unsupported fields.
///
/// Returns `true` if the schema is effectively nullable (contains null type).
#[allow(clippy::single_match)]
pub(super) fn clean_json_schema_recursive(value: &mut Value) -> bool {
    let mut is_effectively_nullable = false;

    match value {
        Value::Object(map) => {
            merge_all_of(map);

            if let Some(Value::Object(props)) = map.get_mut("properties") {
                let mut nullable_keys = HashSet::new();
                for (k, v) in props {
                    if clean_json_schema_recursive(v) {
                        let _ = nullable_keys.insert(k.clone());
                    }
                }

                if !nullable_keys.is_empty() {
                    if let Some(Value::Array(req_arr)) = map.get_mut("required") {
                        req_arr.retain(|r| {
                            r.as_str().map(|s| !nullable_keys.contains(s)).unwrap_or(true)
                        });
                        if req_arr.is_empty() {
                            let _ = map.remove("required");
                        }
                    }
                }
            } else if let Some(items) = map.get_mut("items") {
                let _ = clean_json_schema_recursive(items);
            } else {
                for v in map.values_mut() {
                    let _ = clean_json_schema_recursive(v);
                }
            }

            if let Some(Value::Array(any_of)) = map.get_mut("anyOf") {
                for branch in any_of.iter_mut() {
                    let _ = clean_json_schema_recursive(branch);
                }
            }
            if let Some(Value::Array(one_of)) = map.get_mut("oneOf") {
                for branch in one_of.iter_mut() {
                    let _ = clean_json_schema_recursive(branch);
                }
            }

            let mut union_to_merge = None;
            if map.get("type").is_none()
                || map.get("type").and_then(|t| t.as_str()) == Some("object")
            {
                if let Some(Value::Array(any_of)) = map.get("anyOf") {
                    union_to_merge = Some(any_of.clone());
                } else if let Some(Value::Array(one_of)) = map.get("oneOf") {
                    union_to_merge = Some(one_of.clone());
                }
            }

            if let Some(union_array) = union_to_merge {
                if let Some(Value::Object(branch_obj)) =
                    extract_best_schema_from_union(&union_array)
                {
                    for (k, v) in branch_obj {
                        if k == "properties" {
                            if let Some(target_props) = map
                                .entry("properties".to_string())
                                .or_insert_with(|| Value::Object(serde_json::Map::new()))
                                .as_object_mut()
                            {
                                if let Some(source_props) = v.as_object() {
                                    for (pk, pv) in source_props {
                                        let _ = target_props
                                            .entry(pk.clone())
                                            .or_insert_with(|| pv.clone());
                                    }
                                }
                            }
                        } else if k == "required" {
                            if let Some(target_req) = map
                                .entry("required".to_string())
                                .or_insert_with(|| Value::Array(Vec::new()))
                                .as_array_mut()
                            {
                                if let Some(source_req) = v.as_array() {
                                    for rv in source_req {
                                        if !target_req.contains(rv) {
                                            target_req.push(rv.clone());
                                        }
                                    }
                                }
                            }
                        } else if !map.contains_key(&k) {
                            let _ = map.insert(k, v);
                        }
                    }
                }
            }

            let looks_like_schema = map.contains_key("type")
                || map.contains_key("properties")
                || map.contains_key("items")
                || map.contains_key("enum")
                || map.contains_key("anyOf")
                || map.contains_key("oneOf")
                || map.contains_key("allOf");

            if looks_like_schema {
                let mut hints = Vec::new();
                let constraints = [
                    ("minLength", "minLen"),
                    ("maxLength", "maxLen"),
                    ("pattern", "pattern"),
                    ("minimum", "min"),
                    ("maximum", "max"),
                    ("multipleOf", "multipleOf"),
                    ("exclusiveMinimum", "exclMin"),
                    ("exclusiveMaximum", "exclMax"),
                    ("minItems", "minItems"),
                    ("maxItems", "maxItems"),
                    ("propertyNames", "propertyNames"),
                    ("format", "format"),
                ];
                for (field, label) in constraints {
                    if let Some(val) = map.get(field) {
                        if !val.is_null() {
                            let val_str = if let Some(s) = val.as_str() {
                                s.to_string()
                            } else {
                                val.to_string()
                            };
                            hints.push(format!("{}: {}", label, val_str));
                        }
                    }
                }
                if !hints.is_empty() {
                    let suffix = format!(" [Constraint: {}]", hints.join(", "));
                    let desc_val = map
                        .entry("description".to_string())
                        .or_insert_with(|| Value::String(String::new()));
                    if let Value::String(s) = desc_val {
                        if !s.contains(&suffix) {
                            s.push_str(&suffix);
                        }
                    }
                }

                let allowed_fields = HashSet::from([
                    "type",
                    "description",
                    "properties",
                    "required",
                    "items",
                    "enum",
                    "title",
                ]);
                let keys_to_remove: Vec<String> =
                    map.keys().filter(|k| !allowed_fields.contains(k.as_str())).cloned().collect();
                for k in keys_to_remove {
                    let _ = map.remove(&k);
                }

                if map.get("type").and_then(|t| t.as_str()) == Some("object")
                    && !map.contains_key("properties")
                {
                    let _ = map.insert("properties".to_string(), serde_json::json!({}));
                }

                let valid_prop_keys: Option<HashSet<String>> = map
                    .get("properties")
                    .and_then(|p| p.as_object())
                    .map(|obj| obj.keys().cloned().collect());

                if let Some(required_val) = map.get_mut("required") {
                    if let Some(req_arr) = required_val.as_array_mut() {
                        if let Some(keys) = &valid_prop_keys {
                            req_arr
                                .retain(|k| k.as_str().map(|s| keys.contains(s)).unwrap_or(false));
                        } else {
                            req_arr.clear();
                        }
                    }
                }

                if let Some(type_val) = map.get_mut("type") {
                    let mut selected_type = None;
                    match type_val {
                        Value::String(s) => {
                            let lower = s.to_lowercase();
                            if lower == "null" {
                                is_effectively_nullable = true;
                            } else {
                                selected_type = Some(lower);
                            }
                        },
                        Value::Array(arr) => {
                            for item in arr {
                                if let Value::String(s) = item {
                                    let lower = s.to_lowercase();
                                    if lower == "null" {
                                        is_effectively_nullable = true;
                                    } else if selected_type.is_none() {
                                        selected_type = Some(lower);
                                    }
                                }
                            }
                        },
                        _ => {},
                    }
                    *type_val =
                        Value::String(selected_type.unwrap_or_else(|| "string".to_string()));
                }

                if is_effectively_nullable {
                    let desc_val = map
                        .entry("description".to_string())
                        .or_insert_with(|| Value::String(String::new()));
                    if let Value::String(s) = desc_val {
                        if !s.contains("nullable") {
                            if !s.is_empty() {
                                s.push(' ');
                            }
                            s.push_str("(nullable)");
                        }
                    }
                }

                if let Some(Value::Array(arr)) = map.get_mut("enum") {
                    for item in arr {
                        if !item.is_string() {
                            *item = Value::String(if item.is_null() {
                                "null".to_string()
                            } else {
                                item.to_string()
                            });
                        }
                    }
                }
            }
        },
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                let _ = clean_json_schema_recursive(item);
            }
        },
        _ => {},
    }

    is_effectively_nullable
}
