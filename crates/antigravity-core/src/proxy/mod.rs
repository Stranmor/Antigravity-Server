//! Proxy module - API reverse proxy service
//!
//! This module provides a full-featured API proxy server with:
//! - OpenAI, Claude, Gemini protocol support
//! - Model mapping and routing
//! - Rate limiting and session management
//! - Request monitoring and logging
//!
//! ## Architecture (Post-Symlink Era)
//!
//! All modules are now local copies from vendor/antigravity-upstream.
//! All modules are clippy-clean (Phase 3c completed 2026-01-17).

// ============================================================================
// ALL MODULES (clippy-clean, -D warnings compliant)
// ============================================================================

// Our custom modules
pub mod adaptive_limit;
pub mod health;
pub mod monitor;
pub mod prometheus;
pub mod security;
pub mod server;
pub mod sticky_config;
pub mod token_manager;

// Cleaned upstream modules (Phase 3c complete)
pub mod audio;
pub mod common;
pub mod handlers;
pub mod mappers;
pub mod middleware;
pub mod project_resolver;
pub mod providers;
pub mod rate_limit;
pub mod session_manager;
pub mod signature_cache;
pub mod upstream;
pub mod warp_isolation;
pub mod zai_vision_mcp;
pub mod zai_vision_tools;

// ============================================================================
// RE-EXPORTS
// ============================================================================

// Config types from shared (which re-exports from types)
pub use antigravity_shared::proxy::config;
pub use antigravity_shared::proxy::config::{ProxyAuthMode, ZaiConfig, ZaiDispatchMode};

// Core types
pub use monitor::{ProxyEventBus, ProxyMonitor};
pub use security::ProxySecurityConfig;
pub use server::{
    build_proxy_router, build_proxy_router_with_shared_state, AxumServer, ServerStartConfig,
};
pub use signature_cache::SignatureCache;
pub use token_manager::TokenManager;
pub use warp_isolation::WarpIsolationManager;

// AIMD rate limiting types
pub use adaptive_limit::{
    AIMDController, AdaptiveLimitManager, AdaptiveLimitTracker, AimdAccountStats, ProbeStrategy,
};
pub use common::circuit_breaker::{CircuitBreakerManager, CircuitState};
pub use health::HealthMonitor;

#[cfg(test)]
pub mod tests;
