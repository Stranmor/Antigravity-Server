//! # Antigravity Types
//!
//! Core types, models, and error definitions for Antigravity Manager.
//!
//! This crate provides the foundational type system for the Antigravity ecosystem:
//!
//! - **`error`** - Typed error hierarchy for accounts, proxy, and configuration
//! - **`models`** - Domain models (Account, Config, Quota, Token)
//! - **`protocol`** - OpenAI/Claude/Gemini protocol message types
//!
//! ## Architecture Role
//!
//! `antigravity-types` sits at the bottom of the dependency graph:
//!
//! ```text
//!                antigravity-types (this crate)
//!                        │
//!       ┌────────────────┼────────────────┐
//!       ▼                ▼                ▼
//! antigravity-proxy  antigravity-accounts  ...
//!       │                │
//!       └────────┬───────┘
//!                ▼
//!         antigravity-server
//! ```
//!
//! All types are designed to be:
//! - **Serializable** via serde for API/IPC
//! - **Clone** for cheap sharing across async boundaries
//! - **PartialEq** for testing and comparison

pub mod error;
pub mod models;
pub mod protocol;

// Re-export error types for convenience
pub use error::{AccountError, ConfigError, ProxyError, Result, TypedError};

// Re-export core model types
pub use models::{
    Account, AccountIndex, AccountSummary, AppConfig, ModelQuota, ProxyConfig, ProxyRequestLog,
    ProxyStats, QuotaData, TokenData,
};
