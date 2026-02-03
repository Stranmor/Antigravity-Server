use crate::modules::account::get_data_dir;
use std::fs;
use std::path::PathBuf;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// Custom local timezone time formatter
struct LocalTimer;

impl tracing_subscriber::fmt::time::FormatTime for LocalTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now = chrono::Local::now();
        write!(w, "{}", now.to_rfc3339())
    }
}

pub fn get_log_dir() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let log_dir = data_dir.join("logs");

    if !log_dir.exists() {
        fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Failed to create log directory: {}", e))?;
    }

    Ok(log_dir)
}

/// Initialize the logging system.
pub fn init_logger() {
    // Capture log macro output
    let _ = tracing_log::LogTracer::init();

    let log_dir = match get_log_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("Failed to initialize log directory: {}", e);
            return;
        }
    };

    // 1. Set up file appender with daily rolling
    let file_appender = tracing_appender::rolling::daily(log_dir, "app.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // 2. Console output layer with local timezone
    let console_layer = fmt::Layer::new()
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .with_timer(LocalTimer);

    // 3. File output layer (no ANSI, local timezone)
    let file_layer = fmt::Layer::new()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .with_timer(LocalTimer);

    // 4. Filter layer (default INFO to reduce log volume)
    let filter_layer = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // 5. Initialize global subscriber (use try_init to avoid panic on duplicate init)
    let _ = tracing_subscriber::registry()
        .with(filter_layer)
        .with(console_layer)
        .with(file_layer)
        .try_init();

    // Leak _guard to ensure its lifetime extends to program exit
    // This is the recommended approach when using tracing_appender::non_blocking
    std::mem::forget(_guard);

    info!("Logger initialized (console + file persistence)");

    // Auto-cleanup logs older than 7 days
    if let Err(e) = cleanup_old_logs(7) {
        warn!("Failed to cleanup old logs: {}", e);
    }
}

/// Clean up log files older than the specified number of days.
pub fn cleanup_old_logs(days_to_keep: u64) -> Result<(), String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let log_dir = get_log_dir()?;
    if !log_dir.exists() {
        return Ok(());
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Failed to get system time: {}", e))?
        .as_secs();

    let cutoff_time = now.saturating_sub(days_to_keep * 24 * 60 * 60);
    let mut deleted_count = 0;
    let mut total_size_freed = 0u64;

    let entries =
        fs::read_dir(&log_dir).map_err(|e| format!("Failed to read log directory: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process files, skip directories
        if !path.is_file() {
            continue;
        }

        // Get file modification time
        if let Ok(metadata) = fs::metadata(&path) {
            if let Ok(modified) = metadata.modified() {
                let modified_secs = modified
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                // If file is older than cutoff, delete it
                if modified_secs < cutoff_time {
                    let file_size = metadata.len();
                    if let Err(e) = fs::remove_file(&path) {
                        warn!("Failed to delete old log file {:?}: {}", path, e);
                    } else {
                        deleted_count += 1;
                        total_size_freed += file_size;
                        info!("Deleted old log file: {:?}", path.file_name());
                    }
                }
            }
        }
    }

    if deleted_count > 0 {
        let size_mb = total_size_freed as f64 / 1024.0 / 1024.0;
        info!(
            "Log cleanup complete: deleted {} files, freed {:.2} MB",
            deleted_count, size_mb
        );
    }

    Ok(())
}

/// Clear log cache (uses truncation to keep file handles valid).
pub fn clear_logs() -> Result<(), String> {
    let log_dir = get_log_dir()?;
    if log_dir.exists() {
        // Iterate through all files and truncate instead of deleting directory
        let entries =
            fs::read_dir(&log_dir).map_err(|e| format!("Failed to read log directory: {}", e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                // Open file in truncate mode to set size to 0
                let _ = fs::OpenOptions::new().write(true).truncate(true).open(path);
            }
        }
    }
    Ok(())
}

/// Log info message (backward compatibility interface).
pub fn log_info(message: &str) {
    info!("{}", message);
}

/// Log warning message (backward compatibility interface).
pub fn log_warn(message: &str) {
    warn!("{}", message);
}

/// Log error message (backward compatibility interface).
pub fn log_error(message: &str) {
    error!("{}", message);
}
