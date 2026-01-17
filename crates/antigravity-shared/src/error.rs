//! Typed error definitions for Antigravity.
//!
//! This module re-exports all error types from `antigravity-types`.
//! It exists for backwards compatibility - new code should import from
//! `antigravity-types` directly.
//!
//! ## Migration Note
//!
//! Previously, this module contained duplicate definitions of error types.
//! As of the Phase 3 consolidation, this is now a pure re-export layer.

pub use antigravity_types::error::{AccountError, ConfigError, ProxyError, Result, TypedError};
