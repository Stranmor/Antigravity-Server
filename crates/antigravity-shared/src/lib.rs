//! Antigravity Shared Types
//!
//! This crate provides shared types for the Antigravity Manager.
//! It re-exports core types from `antigravity-types` and adds
//! additional types specific to the shared layer.
//!
//! ## Migration Note
//!
//! Types are being consolidated into `antigravity-types`. This crate
//! will eventually become a thin re-export layer or be deprecated.
//! New code should prefer importing from `antigravity-types` directly.

pub mod error;
pub mod models;
pub mod proxy;
pub mod utils;

// Re-export all types from antigravity-types for backwards compatibility
// New code should prefer importing from antigravity_types directly
pub use antigravity_types::{
    // Error types
    error::{AccountError, ConfigError, ProxyError, TypedError},
    // Protocol types
    protocol,
};
