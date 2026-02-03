//! Path utilities for account data storage.

use std::fs;
use std::path::PathBuf;

/// Directory name for data storage.
pub const DATA_DIR: &str = ".antigravity_tools";
/// Filename for the account index.
pub const ACCOUNTS_INDEX: &str = "accounts.json";
/// Directory name for individual account files.
pub const ACCOUNTS_DIR: &str = "accounts";

/// Get the data directory path.
///
/// Priority:
/// 1. `ANTIGRAVITY_DATA_DIR` environment variable (for container deployments)
/// 2. `~/.antigravity_tools` (default for desktop usage)
pub fn get_data_dir() -> Result<PathBuf, String> {
    let data_dir = if let Ok(custom_dir) = std::env::var("ANTIGRAVITY_DATA_DIR") {
        PathBuf::from(custom_dir)
    } else {
        let home = dirs::home_dir().ok_or("Cannot get home directory")?;
        home.join(DATA_DIR)
    };

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;
    }

    Ok(data_dir)
}

/// Get the accounts directory path.
pub fn get_accounts_dir() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let accounts_dir = data_dir.join(ACCOUNTS_DIR);

    if !accounts_dir.exists() {
        fs::create_dir_all(&accounts_dir)
            .map_err(|e| format!("Failed to create accounts directory: {}", e))?;
    }

    Ok(accounts_dir)
}
