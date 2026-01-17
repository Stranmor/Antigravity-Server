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
//! Clippy warnings are being fixed incrementally - modules marked with
//! #[allow(warnings)] still need cleanup.

// ============= COPIED FROM UPSTREAM (CLIPPY CLEANUP IN PROGRESS) =============
// These modules still have clippy warnings that need to be fixed.
// As each module is cleaned up, its #[allow(warnings)] can be removed.

#[allow(warnings)]
pub mod audio;
#[allow(warnings)]
pub mod handlers;
#[allow(warnings)]
pub mod mappers;
#[allow(warnings)]
pub mod middleware;
#[allow(warnings)]
pub mod project_resolver;
#[allow(warnings)]
pub mod providers;
#[allow(warnings)]
pub mod rate_limit;
#[allow(warnings)]
pub mod security;
#[allow(warnings)]
pub mod session_manager;
#[allow(warnings)]
pub mod signature_cache;
#[allow(warnings)]
pub mod sticky_config;
#[allow(warnings)]
pub mod upstream;
#[allow(warnings)]
pub mod zai_vision_mcp;
#[allow(warnings)]
pub mod zai_vision_tools;

// Common utilities (also needs cleanup)
#[allow(warnings)]
pub mod common;

// ============= OUR CUSTOM MODULES =============
// These are original files maintained by us - CLIPPY STRICT (no allows!)
pub mod adaptive_limit;
pub mod health;
pub mod monitor;
pub mod prometheus;
pub mod server;
pub mod smart_prober;
pub mod token_manager;

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
pub use adaptive_limit::{
    AIMDController, AdaptiveLimitManager, AdaptiveLimitTracker, ProbeStrategy,
};
pub use common::circuit_breaker::{CircuitBreakerManager, CircuitState};
pub use health::HealthMonitor;
pub use smart_prober::SmartProber;

#[cfg(test)]
pub mod tests;
