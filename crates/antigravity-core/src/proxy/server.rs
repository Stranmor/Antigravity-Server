use crate::proxy::TokenManager;
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    routing::{any, get, post},
    Router,
};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

/// Axum 应用状态
#[derive(Clone)]
pub struct AppState {
    pub token_manager: Arc<TokenManager>,
    pub custom_mapping: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    pub upstream_proxy:
        Arc<tokio::sync::RwLock<antigravity_shared::utils::http::UpstreamProxyConfig>>,
    pub security_config: Arc<RwLock<crate::proxy::ProxySecurityConfig>>,
    pub zai: Arc<RwLock<antigravity_shared::proxy::config::ZaiConfig>>,
    pub monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
    pub experimental: Arc<RwLock<antigravity_shared::proxy::config::ExperimentalConfig>>,
    pub adaptive_limits: Arc<crate::proxy::AdaptiveLimitManager>,
    pub health_monitor: Arc<crate::proxy::HealthMonitor>,
    pub circuit_breaker: Arc<crate::proxy::CircuitBreakerManager>,
    pub request_timeout: u64,
    pub thought_signature_map: Arc<tokio::sync::Mutex<std::collections::HashMap<String, String>>>,
    pub upstream: Arc<crate::proxy::upstream::client::UpstreamClient>,
    pub provider_rr: Arc<AtomicUsize>,
    pub zai_vision_mcp: Arc<crate::proxy::zai_vision_mcp::ZaiVisionMcpState>,
}

/// Build proxy router with shared state references for hot-reload support.
/// Unlike `build_proxy_router`, this version accepts pre-created Arc references
/// so that external code can update the mapping at runtime.
#[allow(clippy::too_many_arguments)]
pub fn build_proxy_router_with_shared_state(
    token_manager: Arc<TokenManager>,
    custom_mapping: Arc<tokio::sync::RwLock<std::collections::HashMap<String, String>>>,
    upstream_proxy: antigravity_shared::utils::http::UpstreamProxyConfig,
    security_config: Arc<RwLock<crate::proxy::ProxySecurityConfig>>,
    zai: Arc<RwLock<antigravity_shared::proxy::config::ZaiConfig>>,
    monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
    experimental: Arc<RwLock<antigravity_shared::proxy::config::ExperimentalConfig>>,
    adaptive_limits: Arc<crate::proxy::AdaptiveLimitManager>,
    health_monitor: Arc<crate::proxy::HealthMonitor>,
    circuit_breaker: Arc<crate::proxy::CircuitBreakerManager>,
) -> Router<()> {
    let proxy_state = Arc::new(tokio::sync::RwLock::new(upstream_proxy.clone()));
    let provider_rr = Arc::new(AtomicUsize::new(0));
    let zai_vision_mcp_state = Arc::new(crate::proxy::zai_vision_mcp::ZaiVisionMcpState::new());

    let state = AppState {
        token_manager,
        custom_mapping: custom_mapping.clone(),
        request_timeout: 300,
        thought_signature_map: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        upstream_proxy: proxy_state.clone(),
        upstream: Arc::new(crate::proxy::upstream::client::UpstreamClient::new(Some(
            upstream_proxy,
        ))),
        zai,
        provider_rr,
        zai_vision_mcp: zai_vision_mcp_state,
        monitor,
        experimental,
        adaptive_limits,
        health_monitor,
        circuit_breaker,
        security_config: security_config.clone(),
    };

    use crate::proxy::handlers;

    Router::new()
        // OpenAI Protocol
        .route("/v1/models", get(handlers::openai::handle_list_models))
        .route(
            "/v1/chat/completions",
            post(handlers::openai::handle_chat_completions),
        )
        .route(
            "/v1/completions",
            post(handlers::openai::handle_completions),
        )
        .route("/v1/responses", post(handlers::openai::handle_completions))
        .route(
            "/v1/images/generations",
            post(handlers::openai::handle_images_generations),
        )
        .route(
            "/v1/images/edits",
            post(handlers::openai::handle_images_edits),
        )
        .route(
            "/v1/audio/transcriptions",
            post(handlers::audio::handle_audio_transcription),
        )
        // Claude Protocol
        .route("/v1/messages", post(handlers::claude::handle_messages))
        .route(
            "/v1/messages/count_tokens",
            post(handlers::claude::handle_count_tokens),
        )
        .route(
            "/v1/models/claude",
            get(handlers::claude::handle_list_models),
        )
        // z.ai MCP
        .route(
            "/mcp/web_search_prime/mcp",
            any(handlers::mcp::handle_web_search_prime),
        )
        .route("/mcp/web_reader/mcp", any(handlers::mcp::handle_web_reader))
        .route(
            "/mcp/zai-mcp-server/mcp",
            any(handlers::mcp::handle_zai_mcp_server),
        )
        // Gemini Protocol
        .route("/v1beta/models", get(handlers::gemini::handle_list_models))
        .route(
            "/v1beta/models/:model",
            get(handlers::gemini::handle_get_model).post(handlers::gemini::handle_generate),
        )
        .route(
            "/v1beta/models/:model/countTokens",
            post(handlers::gemini::handle_count_tokens),
        )
        // Utility
        .route(
            "/v1/models/detect",
            post(handlers::common::handle_detect_model),
        )
        .route(
            "/v1/api/event_logging/batch",
            post(|| async { StatusCode::OK }),
        )
        .route("/v1/api/event_logging", post(|| async { StatusCode::OK }))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::proxy::middleware::monitor::monitor_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(axum::middleware::from_fn_with_state(
            security_config,
            crate::proxy::middleware::auth_middleware,
        ))
        .with_state(state)
}

// ===== API 处理器 (旧代码已移除，由 src/proxy/handlers/* 接管) =====
/// Configuration for starting the Axum server
pub struct ServerStartConfig {
    pub host: String,
    pub port: u16,
    pub token_manager: Arc<TokenManager>,
    pub custom_mapping: std::collections::HashMap<String, String>,
    pub upstream_proxy: antigravity_shared::utils::http::UpstreamProxyConfig,
    pub security_config: crate::proxy::ProxySecurityConfig,
    pub zai: antigravity_shared::proxy::config::ZaiConfig,
    pub monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
    pub experimental: antigravity_shared::proxy::config::ExperimentalConfig,
    pub adaptive_limits: Arc<crate::proxy::AdaptiveLimitManager>,
    pub health_monitor: Arc<crate::proxy::HealthMonitor>,
    pub circuit_breaker: Arc<crate::proxy::CircuitBreakerManager>,
}

/// Axum 服务器实例
pub struct AxumServer {
    config: ServerStartConfig,
}

impl AxumServer {
    pub fn new(config: ServerStartConfig) -> Self {
        Self { config }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        tracing::info!("Starting Axum server on {}", addr);

        let custom_mapping = Arc::new(tokio::sync::RwLock::new(self.config.custom_mapping));
        let security_config = Arc::new(RwLock::new(self.config.security_config));
        let zai = Arc::new(RwLock::new(self.config.zai));
        let experimental = Arc::new(RwLock::new(self.config.experimental));

        let app = build_proxy_router_with_shared_state(
            self.config.token_manager,
            custom_mapping,
            self.config.upstream_proxy,
            security_config,
            zai,
            self.config.monitor,
            experimental,
            self.config.adaptive_limits,
            self.config.health_monitor,
            self.config.circuit_breaker,
        );

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// Helper for backward compatibility or simpler usage
#[allow(clippy::too_many_arguments)]
pub fn build_proxy_router(
    token_manager: Arc<TokenManager>,
    custom_mapping: std::collections::HashMap<String, String>,
    upstream_proxy: antigravity_shared::utils::http::UpstreamProxyConfig,
    security_config: crate::proxy::ProxySecurityConfig,
    zai: antigravity_shared::proxy::config::ZaiConfig,
    monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
    experimental: antigravity_shared::proxy::config::ExperimentalConfig,
    adaptive_limits: Arc<crate::proxy::AdaptiveLimitManager>,
    health_monitor: Arc<crate::proxy::HealthMonitor>,
    circuit_breaker: Arc<crate::proxy::CircuitBreakerManager>,
) -> Router<()> {
    let custom_mapping_state = Arc::new(tokio::sync::RwLock::new(custom_mapping));
    let security_state = Arc::new(RwLock::new(security_config));
    let zai_state = Arc::new(RwLock::new(zai));
    let experimental_state = Arc::new(RwLock::new(experimental));

    build_proxy_router_with_shared_state(
        token_manager,
        custom_mapping_state,
        upstream_proxy,
        security_state,
        zai_state,
        monitor,
        experimental_state,
        adaptive_limits,
        health_monitor,
        circuit_breaker,
    )
}
