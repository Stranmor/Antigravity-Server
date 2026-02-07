use serde_json;
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use crate::models::AppConfig;
use crate::modules::account::get_data_dir;

const CONFIG_FILE: &str = "gui_config.json";

/// In-memory config cache with TTL to avoid disk I/O on hot paths.
/// The cache is invalidated on save/update operations.
const CONFIG_CACHE_TTL: Duration = Duration::from_secs(30);

struct CachedConfig {
    config: AppConfig,
    loaded_at: Instant,
}

static CONFIG_CACHE: OnceLock<RwLock<Option<CachedConfig>>> = OnceLock::new();

fn cache_lock() -> &'static RwLock<Option<CachedConfig>> {
    CONFIG_CACHE.get_or_init(|| RwLock::new(None))
}

/// Load config from in-memory cache (TTL = 30s).
///
/// This is the preferred method for hot paths (e.g., token selection).
/// Falls back to disk read on cache miss or expiry.
pub fn load_config_cached() -> Result<AppConfig, String> {
    // Fast path: check read lock
    if let Ok(guard) = cache_lock().read() {
        if let Some(ref cached) = *guard {
            if cached.loaded_at.elapsed() < CONFIG_CACHE_TTL {
                return Ok(cached.config.clone());
            }
        }
    }

    // Slow path: reload from disk and update cache
    let config = load_config()?;

    if let Ok(mut guard) = cache_lock().write() {
        *guard = Some(CachedConfig { config: config.clone(), loaded_at: Instant::now() });
    }

    Ok(config)
}

/// Invalidate the in-memory config cache.
/// Called automatically by `save_config()` and `update_config()`.
pub fn invalidate_config_cache() {
    if let Ok(mut guard) = cache_lock().write() {
        *guard = None;
    }
}

/// Load application configuration from disk.
///
/// Note: This function includes migration logic that automatically migrates
/// legacy mapping fields to custom_mapping.
/// For hot paths, prefer `load_config_cached()` which avoids repeated disk reads.
pub fn load_config() -> Result<AppConfig, String> {
    let data_dir = get_data_dir()?;
    let config_path = data_dir.join(CONFIG_FILE);

    if !config_path.exists() {
        return Ok(AppConfig::new());
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config file: {}", e))?;

    let mut v: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse config file: {}", e))?;

    let mut modified = false;

    // Migration logic
    if let Some(proxy) = v.get_mut("proxy") {
        let mut custom_mapping =
            proxy.get("custom_mapping").and_then(|m| m.as_object()).cloned().unwrap_or_default();

        // Migrate Anthropic mapping
        if let Some(anthropic) = proxy.get_mut("anthropic_mapping").and_then(|m| m.as_object_mut())
        {
            for (k, v) in anthropic.iter() {
                if !k.ends_with("-series") && !custom_mapping.contains_key(k) {
                    custom_mapping.insert(k.clone(), v.clone());
                }
            }
            // Remove legacy field
            if let Some(obj) = proxy.as_object_mut() {
                obj.remove("anthropic_mapping");
            }
            modified = true;
        }

        // Migrate OpenAI mapping
        if let Some(openai) = proxy.get_mut("openai_mapping").and_then(|m| m.as_object_mut()) {
            for (k, v) in openai.iter() {
                if !k.ends_with("-series") && !custom_mapping.contains_key(k) {
                    custom_mapping.insert(k.clone(), v.clone());
                }
            }
            // Remove legacy field
            if let Some(obj) = proxy.as_object_mut() {
                obj.remove("openai_mapping");
            }
            modified = true;
        }

        if modified {
            if let Some(obj) = proxy.as_object_mut() {
                obj.insert("custom_mapping".to_string(), serde_json::Value::Object(custom_mapping));
            }
        }
    }

    let config: AppConfig = serde_json::from_value(v)
        .map_err(|e| format!("Failed to convert config after migration: {}", e))?;

    // If migration occurred, save once to clean up the file
    if modified {
        if let Err(e) = save_config(&config) {
            tracing::warn!("Failed to save migrated config: {}", e);
        }
    }

    Ok(config)
}

/// Save application configuration.
/// Automatically invalidates the in-memory config cache.
pub fn save_config(config: &AppConfig) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let config_path = data_dir.join(CONFIG_FILE);
    let temp_path = data_dir.join(format!("{}.tmp", CONFIG_FILE));

    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    // Atomic write
    fs::write(&temp_path, content).map_err(|e| format!("Failed to write temp config: {}", e))?;
    fs::rename(&temp_path, &config_path).map_err(|e| format!("Failed to save config: {}", e))?;

    // Invalidate cache so next read picks up the new config
    invalidate_config_cache();

    Ok(())
}

/// Update specific fields in the config.
/// Automatically invalidates the in-memory config cache.
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
