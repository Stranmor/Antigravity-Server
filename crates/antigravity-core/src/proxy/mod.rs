//! Proxy module - API reverse proxy service
//!
//! This module provides a full-featured API proxy server with:
//! - OpenAI, Claude, Gemini protocol support
//! - Model mapping and routing
//! - Rate limiting and session management
//! - Request monitoring and logging
//!
//! ## Architecture (Doctrine 2.11d - Symlink Isolation)
//!
//! Upstream modules are symlinked from vendor/antigravity-upstream/
//! Our custom modules are real files in this directory.
//! This allows crate::proxy::* imports to work normally.

// ============= UPSTREAM SYMLINKED MODULES =============
// These are symlinks to vendor/antigravity-upstream/src-tauri/src/proxy/
pub mod audio;
pub mod handlers;
pub mod mappers;
pub mod middleware;
pub mod project_resolver;
pub mod providers;
pub mod rate_limit;
pub mod security;
pub mod session_manager;
pub mod signature_cache;
pub mod sticky_config;
pub mod upstream;
pub mod zai_vision_mcp;
pub mod zai_vision_tools;

// ============= OUR CUSTOM MODULES =============
// These are real files maintained by us
pub mod adaptive_limit;
pub mod health;
pub mod monitor;
pub mod prometheus;
pub mod server;
pub mod smart_prober;
pub mod token_manager;

// ============= MIXED (UPSTREAM + OUR ADDITIONS) =============
// common/ has symlinks to upstream files + our circuit_breaker.rs
pub mod common;

// Re-export config from shared
pub use antigravity_shared::proxy::config;
pub use antigravity_shared::proxy::config::{ProxyAuthMode, ZaiConfig, ZaiDispatchMode};

// Re-export core types
pub use monitor::{ProxyEventBus, ProxyMonitor};
pub use security::ProxySecurityConfig;
pub use server::{build_proxy_router, build_proxy_router_with_shared_state, AxumServer, ServerStartConfig};
pub use signature_cache::SignatureCache;
pub use token_manager::TokenManager;

// Re-export AIMD types
pub use adaptive_limit::{AIMDController, AdaptiveLimitTracker, ProbeStrategy};
pub use common::circuit_breaker::{CircuitBreakerManager, CircuitState};
pub use health::HealthMonitor;
pub use smart_prober::SmartProber;

#[cfg(test)]
pub mod tests;
