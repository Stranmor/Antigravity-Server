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
        },
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                clean_thinking_fields_recursive(v);
            }
        },
        // Intentionally ignored: only Object/Array can contain nested thinking fields
        _ => {},
    }
}
