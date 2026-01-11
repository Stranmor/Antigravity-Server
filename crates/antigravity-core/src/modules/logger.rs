//! Logging utilities.

use tracing_subscriber::{fmt, EnvFilter};

/// Initialize the global logger.
pub fn init_logger() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .init();
}

/// Log info message.
pub fn log_info(msg: &str) {
    tracing::info!("{}", msg);
}

/// Log warning message.
pub fn log_warn(msg: &str) {
    tracing::warn!("{}", msg);
}

/// Log error message.
pub fn log_error(msg: &str) {
    tracing::error!("{}", msg);
}

/// Clear log files (stub - returns Ok for now).
pub fn clear_logs() -> Result<(), String> {
    // TODO: Implement log file clearing
    Ok(())
}
