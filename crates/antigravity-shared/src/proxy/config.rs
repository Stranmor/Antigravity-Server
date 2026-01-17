//! Proxy configuration types.
//!
//! This module re-exports proxy configuration types from `antigravity-types`.
//! All types are now consolidated in the types crate as the single source of truth.
//!
//! ## Migration Note
//!
//! Previously, this module contained duplicate definitions of proxy config types.
//! As of the Phase 3 consolidation, this is now a pure re-export layer.

// Re-export all config types from antigravity-types
pub use antigravity_types::models::{
    // Other config structs
    ExperimentalConfig,
    Protocol,
    // Enums
    ProxyAuthMode,
    ProxyConfig,
    SchedulingMode,
    StickySessionConfig,
    UpstreamProxyConfig,
    // Z.ai config structs
    ZaiConfig,
    ZaiDispatchMode,
    ZaiMcpConfig,
    ZaiModelDefaults,
};

// Re-export UpstreamProxyConfig from utils for backwards compatibility
// (some code may import from utils::http)
