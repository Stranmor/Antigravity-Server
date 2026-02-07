use super::*;
use serde_json::json;

#[test]
fn test_clean_json_schema_draft_2020_12() {
    let mut schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "location": {
                "type": "string",
                "minLength": 1,
                "format": "city"
            },
            "pattern": {
                "type": "object",
                "properties": {
                    "regex": { "type": "string", "pattern": "^[a-z]+$" }
                }
            },
            "unit": {
                "type": ["string", "null"],
                "default": "celsius"
            }
        },
        "required": ["location"]
    });

    clean_json_schema(&mut schema);

    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["location"]["type"], "string");

    assert!(schema["properties"]["location"].get("minLength").is_none());
    assert!(schema["properties"]["location"].get("format").is_none());
    assert!(schema["properties"]["location"]["description"]
        .as_str()
        .unwrap()
        .contains("[Constraint: minLen: 1, format: city]"));

    assert!(schema["properties"].get("pattern").is_some());
    assert_eq!(schema["properties"]["pattern"]["type"], "object");

    assert!(schema["properties"]["pattern"]["properties"]["regex"].get("pattern").is_none());
    assert!(schema["properties"]["pattern"]["properties"]["regex"]["description"]
        .as_str()
        .unwrap()
        .contains("[Constraint: pattern: ^[a-z]+$]"));

    assert_eq!(schema["properties"]["unit"]["type"], "string");
    assert!(schema.get("$schema").is_none());
}

#[test]
fn test_type_fallback() {
    let mut s1 = json!({"type": ["string", "null"]});
    clean_json_schema(&mut s1);
    assert_eq!(s1["type"], "string");

    let mut s2 = json!({"type": ["integer", "null"]});
    clean_json_schema(&mut s2);
    assert_eq!(s2["type"], "integer");
}

#[test]
fn test_flatten_refs() {
    let mut schema = json!({
        "$defs": {
            "Address": {
                "type": "object",
                "properties": {
                    "city": { "type": "string" }
                }
            }
        },
        "properties": {
            "home": { "$ref": "#/$defs/Address" }
        }
    });

    clean_json_schema(&mut schema);

    assert_eq!(schema["properties"]["home"]["type"], "object");
    assert_eq!(schema["properties"]["home"]["properties"]["city"]["type"], "string");
}

#[test]
fn test_clean_json_schema_missing_required() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "existing_prop": { "type": "string" }
        },
        "required": ["existing_prop", "missing_prop"]
    });

    clean_json_schema(&mut schema);

    let required = schema["required"].as_array().unwrap();
    assert_eq!(required.len(), 1);
    assert_eq!(required[0].as_str().unwrap(), "existing_prop");
}

#[test]
fn test_anyof_type_extraction() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "testo": {
                "anyOf": [
                    {"type": "string"},
                    {"type": "null"}
                ],
                "default": null,
                "title": "Testo"
            },
            "importo": {
                "anyOf": [
                    {"type": "number"},
                    {"type": "null"}
                ],
                "default": null,
                "title": "Importo"
            },
            "attivo": {
                "type": "boolean",
                "title": "Attivo"
            }
        }
    });

    clean_json_schema(&mut schema);

    assert!(schema["properties"]["testo"].get("anyOf").is_none());
    assert!(schema["properties"]["importo"].get("anyOf").is_none());

    assert_eq!(schema["properties"]["testo"]["type"], "string");
    assert_eq!(schema["properties"]["importo"]["type"], "number");
    assert_eq!(schema["properties"]["attivo"]["type"], "boolean");

    assert!(schema["properties"]["testo"].get("default").is_none());
}

#[test]
fn test_oneof_type_extraction() {
    let mut schema = json!({
        "properties": {
            "value": {
                "oneOf": [
                    {"type": "integer"},
                    {"type": "null"}
                ]
            }
        }
    });

    clean_json_schema(&mut schema);

    assert!(schema["properties"]["value"].get("oneOf").is_none());
    assert_eq!(schema["properties"]["value"]["type"], "integer");
}

#[test]
fn test_existing_type_preserved() {
    let mut schema = json!({
        "properties": {
            "name": {
                "type": "string",
                "anyOf": [
                    {"type": "number"}
                ]
            }
        }
    });

    clean_json_schema(&mut schema);

    assert_eq!(schema["properties"]["name"]["type"], "string");
    assert!(schema["properties"]["name"].get("anyOf").is_none());
}

#[test]
fn test_issue_815_anyof_properties_preserved() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "config": {
                "anyOf": [
                    {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "recursive": { "type": "boolean" }
                        },
                        "required": ["path"]
                    },
                    { "type": "null" }
                ]
            }
        }
    });

    clean_json_schema(&mut schema);

    let config = &schema["properties"]["config"];

    assert_eq!(config["type"], "object");

    assert!(config.get("properties").is_some());
    assert_eq!(config["properties"]["path"]["type"], "string");
    assert_eq!(config["properties"]["recursive"]["type"], "boolean");

    let req = config["required"].as_array().unwrap();
    assert!(req.iter().any(|v| v == "path"));

    assert!(config.get("anyOf").is_none());

    assert!(config["properties"].get("reason").is_none());
}

#[test]
fn test_clean_json_schema_on_non_schema_object() {
    let mut tool_call = json!({
        "functionCall": {
            "name": "local_shell_call",
            "args": { "command": ["ls"] },
            "id": "call_123"
        }
    });

    clean_json_schema(&mut tool_call);

    let fc = &tool_call["functionCall"];
    assert_eq!(fc["name"], "local_shell_call");
    assert_eq!(fc["args"]["command"][0], "ls");
    assert_eq!(fc["id"], "call_123");
}

#[test]
fn test_nullable_handling_with_description() {
    let mut schema = json!({
        "type": ["string", "null"],
        "description": "User name"
    });

    clean_json_schema(&mut schema);

    assert_eq!(schema["type"], "string");
    assert!(schema["description"].as_str().unwrap().contains("User name"));
    assert!(schema["description"].as_str().unwrap().contains("(nullable)"));
}

#[test]
fn test_infer_object_type_for_array_items_with_properties() {
    let mut schema = json!({
        "type": "array",
        "items": {
            "properties": {
                "name": { "type": "string" },
                "value": { "type": "string" }
            },
            "required": ["name", "value"]
        }
    });

    clean_json_schema(&mut schema);

    assert_eq!(schema["items"]["type"], "object");
    assert!(schema["items"]["properties"].is_object());
}

#[test]
fn test_existing_type_not_overwritten() {
    let mut schema = json!({
        "type": "array",
        "items": {
            "type": "string",
            "properties": {
                "x": { "type": "number" }
            }
        }
    });

    clean_json_schema(&mut schema);

    assert_eq!(schema["items"]["type"], "string");
}

#[test]
fn test_union_merge_works_for_non_object_types() {
    let mut schema = json!({
        "type": "array",
        "anyOf": [
            {
                "type": "array",
                "items": { "type": "string" }
            },
            { "type": "null" }
        ]
    });

    clean_json_schema(&mut schema);

    assert_eq!(schema["type"], "array");
    assert_eq!(schema["items"]["type"], "string");
    assert!(schema.get("anyOf").is_none());
}
