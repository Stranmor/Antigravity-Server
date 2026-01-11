//! Configuration management module.

use std::fs;
use std::path::PathBuf;

use crate::models::AppConfig;
use crate::modules::account::get_data_dir;
use crate::modules::logger;

const CONFIG_FILE: &str = "config.json";

/// Load application configuration from disk.
pub fn load_config() -> Result<AppConfig, String> {
    let data_dir = get_data_dir()?;
    let config_path = data_dir.join(CONFIG_FILE);

    if !config_path.exists() {
        logger::log_info("Config file not found, creating default");
        let default_config = AppConfig::default();
        save_config(&default_config)?;
        return Ok(default_config);
    }

    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("Failed to read config: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))
}

/// Save application configuration to disk.
pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let config_path = data_dir.join(CONFIG_FILE);
    let temp_path = data_dir.join(format!("{}.tmp", CONFIG_FILE));

    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    // Write to temp file first
    fs::write(&temp_path, &content).map_err(|e| format!("Failed to write temp config: {}", e))?;

    // Atomic rename
    fs::rename(&temp_path, &config_path).map_err(|e| format!("Failed to save config: {}", e))?;

    logger::log_info("Config saved successfully");
    Ok(())
}

/// Update specific fields in the config.
pub fn update_config<F>(updater: F) -> Result<AppConfig, String>
where
    F: FnOnce(&mut AppConfig),
{
    let mut config = load_config()?;
    updater(&mut config);
    save_config(&config)?;
    Ok(config)
}

/// Get the data directory path.
pub fn get_data_directory() -> Result<PathBuf, String> {
    get_data_dir()
}
