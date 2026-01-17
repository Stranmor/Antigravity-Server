//! # Antigravity Core
//!
//! Core business logic for Antigravity Manager.
//!
//! ## Architecture (Doctrine 2.11d - Symlink Isolation)
//!
//! ```text
//! antigravity-core/src/proxy/
//! ├── mappers/     → symlink to vendor/antigravity-upstream/.../mappers
//! ├── handlers/    → symlink to vendor/antigravity-upstream/.../handlers
//! ├── common/      → real dir with symlinks + our circuit_breaker.rs
//! ├── server.rs    ← OUR Axum server (real file)
//! ├── token_manager.rs ← OUR implementation (real file)
//! └── adaptive_limit.rs ← OUR AIMD (real file)
//! ```
//!
//! Upstream code lives in vendor/antigravity-upstream/ (git submodule).
//! Symlinks allow crate::proxy::* imports to work normally.

pub mod error;
pub mod models;
pub mod modules;
pub mod proxy;
pub mod utils;

// Re-export commonly used types
pub use error::{AppError, AppResult};
pub use models::{Account, AppConfig, QuotaData, TokenData};
