//! Antigravity Shared Types
//!
//! This crate is a **compatibility layer** that re-exports types from `antigravity-types`.
//! All canonical type definitions now live in `antigravity-types`.
//!
//! ## Migration Status: COMPLETE
//!
//! As of Phase 3 consolidation (2026-01-17), this crate contains NO duplicate
//! type definitions — only re-exports. New code should import from `antigravity-types`
//! directly when possible.
//!
//! ## Structure
//!
//! - `error` - Re-exports `AccountError`, `ProxyError`, `ConfigError`, `TypedError`
//! - `models` - Re-exports `Account`, `AppConfig`, `ProxyConfig`, etc.
//! - `proxy` - Re-exports proxy config types
//! - `utils` - HTTP utilities (re-exports `UpstreamProxyConfig`)

pub mod error;
pub mod models;
pub mod proxy;
pub mod utils;

// Re-export all types from antigravity-types for backwards compatibility
pub use antigravity_types::{
    // Error types
    error::{AccountError, ConfigError, ProxyError, Result, TypedError},
    // Model types
    models::{
        Account, AccountIndex, AccountSummary, AppConfig, ExperimentalConfig, MappingEntry,
        ModelQuota, Protocol, ProxyAuthMode, ProxyConfig, ProxyRequestLog, ProxyStats, QuotaData,
        SchedulingMode, StickySessionConfig, SyncableMapping, TokenData, UpstreamProxyConfig,
        ZaiConfig, ZaiDispatchMode, ZaiMcpConfig, ZaiModelDefaults,
    },
    // Protocol types
    protocol,
};
