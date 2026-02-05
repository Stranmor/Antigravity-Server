use serde_json::Value;

pub trait ToolAdapter: Send + Sync {
    fn matches(&self, tool_name: &str) -> bool;

    fn pre_process(&self, _schema: &mut Value) -> Result<(), String> {
        Ok(())
    }

    fn post_process(&self, _schema: &mut Value) -> Result<(), String> {
        Ok(())
    }
}

pub fn append_hint_to_schema(schema: &mut Value, hint: &str) {
    if let Value::Object(map) = schema {
        let desc_val =
            map.entry("description".to_string()).or_insert_with(|| Value::String(String::new()));

        if let Value::String(s) = desc_val {
            if s.is_empty() {
                *s = hint.to_string();
            } else if !s.contains(hint) {
                *s = format!("{} {}", s, hint);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestAdapter;

    impl ToolAdapter for TestAdapter {
        fn matches(&self, tool_name: &str) -> bool {
            tool_name.starts_with("test__")
        }

        fn pre_process(&self, schema: &mut Value) -> Result<(), String> {
            append_hint_to_schema(schema, "[Test Adapter]");
            Ok(())
        }
    }

    #[test]
    fn test_adapter_matches() {
        let adapter = TestAdapter;
        assert!(adapter.matches("test__function"));
        assert!(!adapter.matches("other__function"));
    }

    #[test]
    fn test_append_hint() {
        let mut schema = json!({"type": "string"});
        append_hint_to_schema(&mut schema, "Test hint");
        assert_eq!(schema["description"], "Test hint");

        append_hint_to_schema(&mut schema, "Another hint");
        assert_eq!(schema["description"], "Test hint Another hint");
    }
}
