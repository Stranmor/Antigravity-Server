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

#![allow(
    clippy::too_many_arguments,
    reason = "Upstream-derived mappers have complex protocol transformation functions"
)]
#![allow(
    clippy::significant_drop_tightening,
    reason = "RwLock guards in async code require careful lifetime management"
)]
#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "Upstream-derived code uses wildcards for forward compatibility"
)]
#![allow(
    clippy::redundant_else,
    reason = "Explicit else blocks improve readability in complex control flow"
)]
#![allow(clippy::map_err_ignore, reason = "Error context is provided in the replacement message")]
#![allow(clippy::implicit_clone, reason = "Explicit .clone() vs .to_string() is stylistic")]
#![allow(
    clippy::redundant_type_annotations,
    reason = "Explicit types improve code clarity in complex async contexts"
)]
#![allow(clippy::needless_continue, reason = "Explicit continue improves loop readability")]
#![allow(
    clippy::branches_sharing_code,
    reason = "Separate branches improve readability even with shared code"
)]
#![allow(
    clippy::derive_partial_eq_without_eq,
    reason = "Some types intentionally don't implement Eq"
)]
// Test-only lints: allow panic!, println!, etc. in test code
#![cfg_attr(
    test,
    allow(
        clippy::panic,
        clippy::print_stdout,
        clippy::float_cmp,
        clippy::unnecessary_join,
        clippy::needless_collect,
        clippy::assertions_on_result_states
    )
)]

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
