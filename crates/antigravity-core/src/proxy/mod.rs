//! Proxy module - API reverse proxy service
//!
//! This module provides a full-featured API proxy server with:
//! - OpenAI, Claude, Gemini protocol support
//! - Model mapping and routing
//! - Rate limiting and session management
//! - Request monitoring and logging

// Core modules
pub mod monitor;
pub mod project_resolver;
pub mod security;
pub mod server;
pub mod token_manager;

// AIMD Predictive Rate Limiting System (restored 2026-01-15)
pub mod adaptive_limit;
pub mod health;
pub mod prometheus;
pub mod smart_prober;

// Handler modules
pub mod audio;
pub mod common;
pub mod handlers;
pub mod mappers;
pub mod middleware;
pub mod providers;
pub mod rate_limit;
pub mod session_manager;
pub mod signature_cache;
pub mod sticky_config;
pub mod upstream;
pub mod zai_vision_mcp;
pub mod zai_vision_tools;

// Re-export config from shared
pub use antigravity_shared::proxy::config;
pub use antigravity_shared::proxy::config::{ProxyAuthMode, ZaiConfig, ZaiDispatchMode};

// Re-export core types
pub use monitor::{ProxyEventBus, ProxyMonitor};
pub use security::ProxySecurityConfig;
pub use server::{
    build_proxy_router, build_proxy_router_with_shared_state, AxumServer, ServerStartConfig,
};
pub use signature_cache::SignatureCache;
pub use token_manager::TokenManager;

// Re-export AIMD types
pub use adaptive_limit::{AIMDController, AdaptiveLimitTracker, ProbeStrategy};
pub use common::circuit_breaker::{CircuitBreakerManager, CircuitState};
pub use health::HealthMonitor;
pub use smart_prober::SmartProber;

#[cfg(test)]
pub mod tests;
