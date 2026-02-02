//! # Antigravity Core
//!
//! Core business logic for Antigravity Manager.
//!
//! ## Architecture (Post-Symlink Era)
//!
//! ```text
//! antigravity-core/src/proxy/
//! ├── mappers/          # LOCAL COPY (from vendor/antigravity-upstream)
//! ├── handlers/         # LOCAL COPY (claude.rs, openai.rs, gemini.rs)
//! ├── common/           # LOCAL COPY + our circuit_breaker.rs
//! ├── server.rs         # OUR Axum server
//! ├── token_manager.rs  # OUR implementation
//! ├── adaptive_limit.rs # OUR AIMD rate limiting
//! ├── health.rs         # OUR health monitoring
//! └── prometheus.rs     # OUR metrics endpoint
//! ```
//!
//! All proxy code is now local copies (no symlinks). Upstream reference lives
//! in vendor/antigravity-upstream/ (git submodule) for feature porting.

// Upstream-derived mappers have complex protocol transformation functions
// with many arguments. Refactoring would diverge from upstream significantly.
#![allow(clippy::too_many_arguments)]

pub mod error;
pub mod models;
pub mod modules;
pub mod proxy;
pub mod utils;

// Re-export commonly used types
pub use error::{AppError, AppResult};
pub use models::{Account, AppConfig, QuotaData, TokenData};
pub use modules::device::{
    backup_storage, generate_profile, get_storage_path, load_global_original, read_profile,
    save_global_original, write_profile,
};
