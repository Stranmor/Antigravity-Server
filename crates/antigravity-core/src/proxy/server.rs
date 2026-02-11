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

/// Axum application state
#[derive(Clone)]
pub struct AppState {
    pub token_manager: Arc<TokenManager>,
    pub custom_mapping: Arc<RwLock<std::collections::HashMap<String, String>>>,
    pub upstream_proxy: Arc<RwLock<antigravity_types::models::UpstreamProxyConfig>>,
    pub security_config: Arc<RwLock<crate::proxy::ProxySecurityConfig>>,
    pub zai: Arc<RwLock<antigravity_types::models::ZaiConfig>>,
    pub monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
    pub experimental: Arc<RwLock<antigravity_types::models::ExperimentalConfig>>,
    pub adaptive_limits: Arc<crate::proxy::AdaptiveLimitManager>,
    pub health_monitor: Arc<crate::proxy::HealthMonitor>,
    pub circuit_breaker: Arc<crate::proxy::CircuitBreakerManager>,
    pub request_timeout: u64,
    pub http_client: reqwest::Client,
    pub upstream: Arc<crate::proxy::upstream::client::UpstreamClient>,
    pub provider_rr: Arc<AtomicUsize>,
    pub zai_vision_mcp: Arc<crate::proxy::zai_vision_mcp::ZaiVisionMcpState>,
}

/// Build proxy router with shared state references for hot-reload support.
///
/// Unlike `build_proxy_router`, this version accepts pre-created Arc references
/// so that external code can update the mapping at runtime.
#[allow(clippy::too_many_arguments, reason = "server bootstrap requires all subsystem references")]
pub fn build_proxy_router_with_shared_state(
    token_manager: Arc<TokenManager>,
    custom_mapping: Arc<RwLock<std::collections::HashMap<String, String>>>,
    upstream_proxy: Arc<RwLock<antigravity_types::models::UpstreamProxyConfig>>,
    security_config: Arc<RwLock<crate::proxy::ProxySecurityConfig>>,
    zai: Arc<RwLock<antigravity_types::models::ZaiConfig>>,
    monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
    experimental: Arc<RwLock<antigravity_types::models::ExperimentalConfig>>,
    adaptive_limits: Arc<crate::proxy::AdaptiveLimitManager>,
    health_monitor: Arc<crate::proxy::HealthMonitor>,
    circuit_breaker: Arc<crate::proxy::CircuitBreakerManager>,
    http_client: reqwest::Client,
) -> Router<()> {
    let provider_rr = Arc::new(AtomicUsize::new(0));
    let zai_vision_mcp_state = Arc::new(crate::proxy::zai_vision_mcp::ZaiVisionMcpState::new());

    let state = AppState {
        token_manager,
        custom_mapping: Arc::clone(&custom_mapping),
        request_timeout: 300,
        http_client: http_client.clone(),
        upstream_proxy: Arc::clone(&upstream_proxy),
        upstream: Arc::new(crate::proxy::upstream::client::UpstreamClient::new(
            http_client,
            Arc::clone(&upstream_proxy),
            None,
        )),
        zai,
        provider_rr,
        zai_vision_mcp: zai_vision_mcp_state,
        monitor,
        experimental,
        adaptive_limits,
        health_monitor,
        circuit_breaker,
        security_config: Arc::clone(&security_config),
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
            "/v1/audio/transcriptions",
            post(handlers::audio::handle_audio_transcription).layer(DefaultBodyLimit::max(
                crate::proxy::audio::AudioProcessor::max_size_bytes(),
            )),
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
            post(handlers::model_detect::handle_detect_model),
        )
        .route(
            "/v1/api/event_logging/batch",
            post(|| async { StatusCode::OK }),
        )
        .route("/v1/api/event_logging", post(|| async { StatusCode::OK }))
        .layer(axum::middleware::from_fn_with_state(
            security_config,
            crate::proxy::middleware::auth_middleware,
        ))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::proxy::middleware::monitor::monitor_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// ===== API handlers (legacy code removed, handled by src/proxy/handlers/*) =====
/// Configuration for starting the Axum server
pub struct ServerStartConfig {
    pub host: String,
    pub port: u16,
    pub token_manager: Arc<TokenManager>,
    pub custom_mapping: std::collections::HashMap<String, String>,
    pub upstream_proxy: antigravity_types::models::UpstreamProxyConfig,
    pub security_config: crate::proxy::ProxySecurityConfig,
    pub zai: antigravity_types::models::ZaiConfig,
    pub monitor: Arc<crate::proxy::monitor::ProxyMonitor>,
    pub experimental: antigravity_types::models::ExperimentalConfig,
    pub adaptive_limits: Arc<crate::proxy::AdaptiveLimitManager>,
    pub health_monitor: Arc<crate::proxy::HealthMonitor>,
    pub circuit_breaker: Arc<crate::proxy::CircuitBreakerManager>,
}

/// Axum server instance
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

        let custom_mapping = Arc::new(RwLock::new(self.config.custom_mapping));
        let security_config = Arc::new(RwLock::new(self.config.security_config));
        let zai = Arc::new(RwLock::new(self.config.zai));
        let experimental = Arc::new(RwLock::new(self.config.experimental));

        let app = build_proxy_router_with_shared_state(
            self.config.token_manager,
            custom_mapping,
            Arc::new(RwLock::new(self.config.upstream_proxy.clone())),
            security_config,
            zai,
            self.config.monitor,
            experimental,
            self.config.adaptive_limits,
            self.config.health_monitor,
            self.config.circuit_breaker,
            reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        );

        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}
