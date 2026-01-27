#[cfg(test)]
mod tests {
    use crate::proxy::common::model_mapping::get_all_dynamic_models;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_get_all_dynamic_models_returns_custom_mapping() {
        let custom_mapping = RwLock::new(HashMap::from([
            ("gpt-4o".to_string(), "gemini-3-pro".to_string()),
            ("my-custom-alias".to_string(), "claude-opus-4-5".to_string()),
        ]));

        let models = get_all_dynamic_models(&custom_mapping).await;

        assert!(models.contains(&"gpt-4o".to_string()));
        assert!(models.contains(&"my-custom-alias".to_string()));
    }

    #[tokio::test]
    async fn test_get_all_dynamic_models_includes_default_models() {
        let custom_mapping = RwLock::new(HashMap::new());

        let models = get_all_dynamic_models(&custom_mapping).await;

        assert!(
            !models.is_empty(),
            "Should include default models even with empty custom mapping"
        );
        assert!(
            models.len() > 10,
            "Should have many built-in models, got {}",
            models.len()
        );
    }

    #[tokio::test]
    async fn test_get_all_dynamic_models_includes_image_models() {
        let custom_mapping = RwLock::new(HashMap::new());

        let models = get_all_dynamic_models(&custom_mapping).await;

        let image_models: Vec<_> = models.iter().filter(|m| m.contains("image")).collect();

        assert!(
            !image_models.is_empty(),
            "Should include image generation models"
        );
    }

    #[tokio::test]
    async fn test_custom_mapping_appears_in_models_list() {
        let custom_mapping = RwLock::new(HashMap::from([
            ("my-special-model".to_string(), "gemini-3-flash".to_string()),
            ("another-alias".to_string(), "claude-sonnet-4-5".to_string()),
        ]));

        let models = get_all_dynamic_models(&custom_mapping).await;

        assert!(models.contains(&"my-special-model".to_string()));
        assert!(models.contains(&"another-alias".to_string()));
    }
}
