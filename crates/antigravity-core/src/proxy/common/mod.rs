//! Common utilities module
//!
//! Combines upstream common modules with our custom extensions

// Upstream modules via #[path]
#[path = "../../../../../vendor/antigravity-upstream/src-tauri/src/proxy/common/json_schema.rs"]
pub mod json_schema;

#[path = "../../../../../vendor/antigravity-upstream/src-tauri/src/proxy/common/model_mapping.rs"]
pub mod model_mapping;

#[path = "../../../../../vendor/antigravity-upstream/src-tauri/src/proxy/common/utils.rs"]
pub mod utils;

// Our custom modules (real files in this directory)
pub mod circuit_breaker;
pub mod model_mapping_ext;

// Re-export key types
pub use circuit_breaker::{CircuitBreakerManager, CircuitState};
// Re-export extended model mapping function
pub use model_mapping_ext::resolve_model_route;

