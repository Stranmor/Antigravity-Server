//! Logging convenience wrappers.
//!
//! Thin wrappers around tracing macros used throughout the crate.

use tracing::{error, info, warn};

/// Log info message (backward compatibility interface).
pub(crate) fn log_info(message: &str) {
    info!("{}", message);
}

/// Log warning message (backward compatibility interface).
pub(crate) fn log_warn(message: &str) {
    warn!("{}", message);
}

/// Log error message (backward compatibility interface).
pub(crate) fn log_error(message: &str) {
    error!("{}", message);
}
