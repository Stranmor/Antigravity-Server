#[cfg(test)]
mod tests {
    #[test]
    fn test_image_model_variants_generation() {
        use crate::proxy::common::model_mapping::generate_image_model_variants;
        let variants = generate_image_model_variants();
        assert_eq!(variants.len(), 21, "3 resolutions × 7 ratios = 21 variants");
        assert!(variants.contains(&"gemini-3-pro-image".to_owned()));
        assert!(variants.contains(&"gemini-3-pro-image-4k-16x9".to_owned()));
        assert!(variants.contains(&"gemini-3-pro-image-2k-1x1".to_owned()));
    }
}

/// Integration tests for HTTP handlers using axum-test
#[cfg(test)]
mod integration_tests {
    use crate::proxy::server::AppState;
    use crate::proxy::{
        AdaptiveLimitManager, CircuitBreakerManager, HealthMonitor, ProxyMonitor,
        ProxySecurityConfig, TokenManager,
    };
    use axum::{routing::get, Router};
    use std::collections::HashMap;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Creates a minimal AppState for testing endpoints that don't require real accounts
    fn create_test_app_state() -> AppState {
        let temp_dir =
            std::env::temp_dir().join(format!("antigravity-test-{}", uuid::Uuid::new_v4()));
        let token_manager = Arc::new(TokenManager::new(temp_dir));
        let custom_mapping = Arc::new(RwLock::new(HashMap::new()));
        let upstream_proxy =
            Arc::new(RwLock::new(antigravity_types::models::UpstreamProxyConfig::default()));
        let security_config = Arc::new(RwLock::new(ProxySecurityConfig {
            auth_mode: antigravity_types::models::ProxyAuthMode::Off,
            api_key: "test-key".to_string(),
            allow_lan_access: true,
        }));
        let zai = Arc::new(RwLock::new(antigravity_types::models::ZaiConfig::default()));
        let monitor = Arc::new(ProxyMonitor::new());
        let experimental =
            Arc::new(RwLock::new(antigravity_types::models::ExperimentalConfig::default()));
        let adaptive_limits = Arc::new(AdaptiveLimitManager::default());
        let health_monitor = HealthMonitor::new();
        let circuit_breaker = Arc::new(CircuitBreakerManager::new());
        let upstream = Arc::new(crate::proxy::upstream::client::UpstreamClient::new(None, None));

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
            upstream,
            provider_rr: Arc::new(AtomicUsize::new(0)),
            zai_vision_mcp: Arc::new(crate::proxy::zai_vision_mcp::ZaiVisionMcpState::new()),
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
            .route("/v1/models", get(crate::proxy::handlers::openai::handle_list_models))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_list_models_endpoint_returns_200() {
        let state = create_test_app_state();
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app).unwrap().get("/v1/models").await;

        response.assert_status_ok();
    }

    #[tokio::test]
    async fn test_list_models_endpoint_returns_json() {
        let state = create_test_app_state();
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app).unwrap().get("/v1/models").await;

        let json: serde_json::Value = response.json();

        assert_eq!(json["object"], "list");
        assert!(json["data"].is_array());
    }

    #[tokio::test]
    async fn test_list_models_includes_default_models() {
        let state = create_test_app_state();
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app).unwrap().get("/v1/models").await;

        let json: serde_json::Value = response.json();
        let data = json["data"].as_array().unwrap();

        assert!(data.len() >= 21, "Expected >=21 image variant models, got {}", data.len());

        let model_ids: Vec<&str> = data.iter().filter_map(|m| m["id"].as_str()).collect();
        assert!(model_ids.contains(&"gemini-3-pro-image"), "Should contain base image model");
        assert!(model_ids.contains(&"gemini-3-pro-image-4k-16x9"), "Should contain image variant");

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
            ("test-model-alias".to_string(), "claude-opus-4-5".to_string()),
        ]);
        let state = create_test_app_state_with_mapping(mapping);
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app).unwrap().get("/v1/models").await;

        let json: serde_json::Value = response.json();
        let data = json["data"].as_array().unwrap();

        let model_ids: Vec<&str> = data.iter().filter_map(|m| m["id"].as_str()).collect();

        assert!(model_ids.contains(&"my-custom-gpt"), "Custom model not found in list");
        assert!(model_ids.contains(&"test-model-alias"), "Test alias not found in list");
        assert!(
            data.len() >= 23,
            "Expected >=23 models (21 image variants + 2 custom mappings), got {}",
            data.len()
        );
    }

    #[tokio::test]
    async fn test_list_models_includes_image_models() {
        let state = create_test_app_state();
        let app = build_models_router(state);

        let response = axum_test::TestServer::new(app).unwrap().get("/v1/models").await;

        let json: serde_json::Value = response.json();
        let data = json["data"].as_array().unwrap();

        let image_models: Vec<&str> = data
            .iter()
            .filter_map(|m| m["id"].as_str())
            .filter(|id| id.contains("image"))
            .collect();

        assert_eq!(
            image_models.len(),
            21,
            "Expected 21 image model variants (3 res × 7 ratios), got {}",
            image_models.len()
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

    fn create_test_app_state_with_auth(
        auth_mode: antigravity_types::models::ProxyAuthMode,
        api_key: &str,
    ) -> AppState {
        let mut state = create_test_app_state();
        state.security_config = Arc::new(RwLock::new(ProxySecurityConfig {
            auth_mode,
            api_key: api_key.to_string(),
            allow_lan_access: true,
        }));
        state
    }

    fn build_models_router_with_auth(state: AppState) -> Router {
        let security_config = state.security_config.clone();
        Router::new()
            .route("/v1/models", get(crate::proxy::handlers::openai::handle_list_models))
            .layer(axum::middleware::from_fn_with_state(
                security_config,
                crate::proxy::middleware::auth_middleware,
            ))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_auth_middleware_rejects_missing_token() {
        let state = create_test_app_state_with_auth(
            antigravity_types::models::ProxyAuthMode::AllExceptHealth,
            "secret-api-key",
        );
        let app = build_models_router_with_auth(state);

        let response = axum_test::TestServer::new(app).unwrap().get("/v1/models").await;

        response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_rejects_invalid_token() {
        let state = create_test_app_state_with_auth(
            antigravity_types::models::ProxyAuthMode::AllExceptHealth,
            "secret-api-key",
        );
        let app = build_models_router_with_auth(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .get("/v1/models")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_static("Bearer wrong-key"),
            )
            .await;

        response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_accepts_valid_token() {
        let state = create_test_app_state_with_auth(
            antigravity_types::models::ProxyAuthMode::AllExceptHealth,
            "secret-api-key",
        );
        let app = build_models_router_with_auth(state);

        let response = axum_test::TestServer::new(app)
            .unwrap()
            .get("/v1/models")
            .add_header(
                axum::http::header::AUTHORIZATION,
                axum::http::HeaderValue::from_static("Bearer secret-api-key"),
            )
            .await;

        response.assert_status_ok();
    }
}
