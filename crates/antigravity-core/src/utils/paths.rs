use std::fs;
use std::path::PathBuf;

const DATA_DIR: &str = ".antigravity_tools";

/// Get data directory path.
///
/// Priority:
/// 1. `ANTIGRAVITY_DATA_DIR` environment variable (for containinger deployments)
/// 2. `~/.antigravity_tools` (default for desktop usage)
pub fn get_data_dir() -> Result<PathBuf, String> {
    let data_dir = if let Ok(custom_dir) = std::env::var("ANTIGRAVITY_DATA_DIR") {
        PathBuf::from(custom_dir)
    } else {
        let home = dirs::home_dir().ok_or("Failed to get user home directory")?;
        home.join(DATA_DIR)
    };

    // ensuredirectoryexist
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("createdatadirectoryfailed: {}", e))?;
    }

    Ok(data_dir)
}
