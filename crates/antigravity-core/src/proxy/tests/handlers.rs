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

/// Integration tests for HTTP handlers using axum-test
#[cfg(test)]
mod integration_tests {
    use crate::proxy::server::AppState;
    use crate::proxy::{
        AdaptiveLimitManager, CircuitBreakerManager, HealthMonitor, ProxyMonitor,
        ProxySecurityConfig, TokenManager, WarpIsolationManager,
    };
    use axum::{routing::get, Router};
    use std::collections::HashMap;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Creates a minimal AppState for testing endpoints that don't require real accounts
    fn create_test_app_state() -> AppState {
        let token_manager = Arc::new(TokenManager::new(std::path::PathBuf::from("/tmp/test")));
        let custom_mapping = Arc::new(RwLock::new(HashMap::new()));
        let upstream_proxy = Arc::new(RwLock::new(
            antigravity_shared::utils::http::UpstreamProxyConfig::default(),
        ));
        let security_config = Arc::new(RwLock::new(ProxySecurityConfig {
            auth_mode: antigravity_shared::proxy::config::ProxyAuthMode::Off,
            api_key: "test-key".to_string(),
            allow_lan_access: true,
        }));
        let zai = Arc::new(RwLock::new(
            antigravity_shared::proxy::config::ZaiConfig::default(),
        ));
        let monitor = Arc::new(ProxyMonitor::new());
        let experimental = Arc::new(RwLock::new(
            antigravity_shared::proxy::config::ExperimentalConfig::default(),
        ));
        let adaptive_limits = Arc::new(AdaptiveLimitManager::default());
        let health_monitor = HealthMonitor::new();
        let circuit_breaker = Arc::new(CircuitBreakerManager::new());
        let upstream = Arc::new(crate::proxy::upstream::client::UpstreamClient::new(None));
        let warp_isolation = Arc::new(WarpIsolationManager::new());

        AppState {
            token_manager,
            custom_mapping,
            upstream_proxy,
            security_config,
            zai,
            monitor,
            experimental,
            adaptive_limits,
            health_monitor,
            circuit_breaker,
            request_timeout: 300,
            thought_signature_map: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            upstream,
            provider_rr: Arc::new(AtomicUsize::new(0)),
            zai_vision_mcp: Arc::new(crate::proxy::zai_vision_mcp::ZaiVisionMcpState::new()),
            warp_isolation,
        }
    }

    /// Creates test AppState with custom model mappings
    fn create_test_app_state_with_mapping(mapping: HashMap<String, String>) -> AppState {
        let mut state = create_test_app_state();
        state.custom_mapping = Arc::new(RwLock::new(mapping));
        state
    }

    /// Builds a test router with only the /v1/models endpoint
    fn build_models_router(state: AppState) -> Router {
        Router::new()
            .route(
                "/v1/models",
                get(crate::proxy::handlers::openai::handle_list_models),
            )
            .with_state(state)
    }

    #[tokio::test]
    async fn test_list_models_endpoint_returns_200() {
        let state = create_test_app_state();
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .get("/v1/models")
            .await;

        response.assert_status_ok();
    }

    #[tokio::test]
    async fn test_list_models_endpoint_returns_json() {
        let state = create_test_app_state();
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .get("/v1/models")
            .await;

        let json: serde_json::Value = response.json();

        assert_eq!(json["object"], "list");
        assert!(json["data"].is_array());
    }

    #[tokio::test]
    async fn test_list_models_includes_default_models() {
        let state = create_test_app_state();
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .get("/v1/models")
            .await;

        let json: serde_json::Value = response.json();
        let data = json["data"].as_array().unwrap();

        assert!(data.len() > 10, "Expected >10 models, got {}", data.len());

        for model in data {
            assert!(model["id"].is_string());
            assert_eq!(model["object"], "model");
            assert!(model["created"].is_number());
            assert_eq!(model["owned_by"], "antigravity");
        }
    }

    #[tokio::test]
    async fn test_list_models_includes_custom_mapping() {
        let mapping = HashMap::from([
            ("my-custom-gpt".to_string(), "gemini-3-pro-high".to_string()),
            (
                "test-model-alias".to_string(),
                "claude-opus-4-5".to_string(),
            ),
        ]);
        let state = create_test_app_state_with_mapping(mapping);
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .get("/v1/models")
            .await;

        let json: serde_json::Value = response.json();
        let data = json["data"].as_array().unwrap();

        let model_ids: Vec<&str> = data.iter().filter_map(|m| m["id"].as_str()).collect();

        assert!(
            model_ids.contains(&"my-custom-gpt"),
            "Custom model not found in list"
        );
        assert!(
            model_ids.contains(&"test-model-alias"),
            "Test alias not found in list"
        );
    }

    #[tokio::test]
    async fn test_list_models_includes_image_models() {
        let state = create_test_app_state();
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .get("/v1/models")
            .await;

        let json: serde_json::Value = response.json();
        let data = json["data"].as_array().unwrap();

        let image_models: Vec<&str> = data
            .iter()
            .filter_map(|m| m["id"].as_str())
            .filter(|id| id.contains("image"))
            .collect();

        assert!(
            !image_models.is_empty(),
            "No image models found in response"
        );
    }

    fn build_chat_completions_router(state: AppState) -> Router {
        Router::new()
            .route(
                "/v1/chat/completions",
                axum::routing::post(crate::proxy::handlers::openai::handle_chat_completions),
            )
            .with_state(state)
    }

    #[tokio::test]
    async fn test_chat_completions_rejects_invalid_json() {
        let state = create_test_app_state();
        let app = build_chat_completions_router(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .post("/v1/chat/completions")
            .content_type("application/json")
            .bytes("not valid json".into())
            .await;

        response.assert_status_bad_request();
    }

    #[tokio::test]
    async fn test_chat_completions_rejects_missing_model() {
        let state = create_test_app_state();
        let app = build_chat_completions_router(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .post("/v1/chat/completions")
            .content_type("application/json")
            .json(&serde_json::json!({
                "messages": [{"role": "user", "content": "Hello"}]
            }))
            .await;

        response.assert_status_bad_request();
    }

    #[tokio::test]
    async fn test_chat_completions_no_accounts_returns_503() {
        let state = create_test_app_state();
        let app = build_chat_completions_router(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .post("/v1/chat/completions")
            .content_type("application/json")
            .json(&serde_json::json!({
                "model": "gemini-3-pro",
                "messages": [{"role": "user", "content": "Hello"}]
            }))
            .await;

        response.assert_status(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }
}
