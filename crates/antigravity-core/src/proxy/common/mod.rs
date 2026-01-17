//! Common utilities module
//!
//! Contains shared utilities for proxy handlers and mappers.
//! All files are now local copies (no longer symlinks to vendor).

// Copied from vendor/antigravity-upstream (now ours to maintain)
pub mod json_schema;
pub mod model_mapping;
pub mod utils;

// Our custom modules
pub mod circuit_breaker;
pub mod model_mapping_ext;

// Re-export key types
pub use circuit_breaker::{CircuitBreakerManager, CircuitState};
// Re-export extended model mapping function
pub use model_mapping_ext::resolve_model_route;
