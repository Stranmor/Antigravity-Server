//! Device fingerprint generation and storage for account isolation.
//!
//! Generates unique device fingerprints (Cursor/VSCode style) to prevent
//! cross-account correlation by upstream APIs. Also provides storage.json
//! reading/writing for profile persistence.

use antigravity_types::models::DeviceProfile;
use chrono::Local;
use rand::Rng;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const DATA_DIR: &str = ".antigravity_tools";
const GLOBAL_BASELINE: &str = "device_original.json";

pub fn generate_profile() -> DeviceProfile {
    DeviceProfile {
        machine_id: format!("auth0|user_{}", random_hex(32)),
        mac_machine_id: new_standard_machine_id(),
        dev_device_id: Uuid::new_v4().to_string(),
        sqm_id: format!("{{{}}}", Uuid::new_v4().to_string().to_uppercase()),
    }
}

fn random_hex(length: usize) -> String {
    const HEX_CHARS: &[u8] = b"0123456789abcdef";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| HEX_CHARS[rng.gen_range(0..16)] as char)
        .collect()
}

fn new_standard_machine_id() -> String {
    let mut rng = rand::thread_rng();
    let mut id = String::with_capacity(36);

    for ch in "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".chars() {
        match ch {
            '-' | '4' => id.push(ch),
            'x' => id.push_str(&format!("{:x}", rng.gen_range(0..16))),
            'y' => id.push_str(&format!("{:x}", rng.gen_range(8..12))),
            _ => unreachable!(),
        }
    }
    id
}

fn get_data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("failed_to_get_home_dir")?;
    let data_dir = home.join(DATA_DIR);
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).map_err(|e| format!("failed_to_create_data_dir: {}", e))?;
    }
    Ok(data_dir)
}

pub fn get_storage_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("failed_to_get_home_dir")?;

    // Linux paths
    let linux_path = home.join(".cursor/User/globalStorage/storage.json");
    if linux_path.exists() {
        return Ok(linux_path);
    }
    let linux_config = home.join(".config/Cursor/User/globalStorage/storage.json");
    if linux_config.exists() {
        return Ok(linux_config);
    }

    // macOS path (Library/Application Support)
    #[cfg(target_os = "macos")]
    {
        let macos_path =
            home.join("Library/Application Support/Cursor/User/globalStorage/storage.json");
        if macos_path.exists() {
            return Ok(macos_path);
        }
    }

    // Windows path (APPDATA)
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = dirs::config_dir() {
            let win_path = appdata.join("Cursor/User/globalStorage/storage.json");
            if win_path.exists() {
                return Ok(win_path);
            }
        }
    }

    Err("storage_json_not_found".to_string())
}

/// Get the directory containing storage.json
fn get_storage_dir() -> Result<PathBuf, String> {
    let path = get_storage_path()?;
    path.parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "failed_to_get_storage_parent_dir".to_string())
}

/// Get state.vscdb path (same directory as storage.json)
pub fn get_state_db_path() -> Result<PathBuf, String> {
    let dir = get_storage_dir()?;
    Ok(dir.join("state.vscdb"))
}

/// Core SQLite sync logic - testable with any db_path
fn sync_to_state_db(db_path: &Path, service_id: &str) -> Result<(), String> {
    if !db_path.exists() {
        tracing::warn!("state_db_missing: {:?}", db_path);
        return Ok(());
    }

    let conn = Connection::open(db_path).map_err(|e| format!("db_open_failed: {}", e))?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS ItemTable (key TEXT PRIMARY KEY, value TEXT);",
        [],
    )
    .map_err(|e| format!("failed_to_create_item_table: {}", e))?;
    conn.execute(
        "INSERT OR REPLACE INTO ItemTable (key, value) VALUES ('storage.serviceMachineId', ?1);",
        [service_id],
    )
    .map_err(|e| format!("failed_to_write_to_db: {}", e))?;

    tracing::info!("service_machine_id_synced_to_db");
    Ok(())
}

/// Sync serviceMachineId to state.vscdb SQLite database
/// Cursor/VSCode may read this value from SQLite, not from storage.json
fn sync_state_service_machine_id_value(service_id: &str) -> Result<(), String> {
    let db_path = get_state_db_path()?;
    sync_to_state_db(&db_path, service_id)
}

pub fn backup_storage(storage_path: &Path) -> Result<PathBuf, String> {
    if !storage_path.exists() {
        return Err(format!("storage_json_missing: {:?}", storage_path));
    }
    let dir = storage_path
        .parent()
        .ok_or_else(|| "failed_to_get_storage_parent_dir".to_string())?;
    let backup_path = dir.join(format!(
        "storage.json.backup_{}",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    fs::copy(storage_path, &backup_path).map_err(|e| format!("backup_failed: {}", e))?;
    Ok(backup_path)
}

pub fn read_profile(storage_path: &Path) -> Result<DeviceProfile, String> {
    let content = fs::read_to_string(storage_path)
        .map_err(|e| format!("read_failed ({:?}): {}", storage_path, e))?;
    let json: Value = serde_json::from_str(&content)
        .map_err(|e| format!("parse_failed ({:?}): {}", storage_path, e))?;

    let get_field = |key: &str| -> Option<String> {
        if let Some(obj) = json.get("telemetry").and_then(|v| v.as_object()) {
            if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
                return Some(v.to_string());
            }
        }
        if let Some(v) = json
            .get(format!("telemetry.{key}"))
            .and_then(|v| v.as_str())
        {
            return Some(v.to_string());
        }
        None
    };

    Ok(DeviceProfile {
        machine_id: get_field("machineId").ok_or("missing_machine_id")?,
        mac_machine_id: get_field("macMachineId").ok_or("missing_mac_machine_id")?,
        dev_device_id: get_field("devDeviceId").ok_or("missing_dev_device_id")?,
        sqm_id: get_field("sqmId").ok_or("missing_sqm_id")?,
    })
}

pub fn write_profile(storage_path: &Path, profile: &DeviceProfile) -> Result<(), String> {
    if !storage_path.exists() {
        return Err(format!("storage_json_missing: {:?}", storage_path));
    }

    let content = fs::read_to_string(storage_path).map_err(|e| format!("read_failed: {}", e))?;
    let mut json: Value =
        serde_json::from_str(&content).map_err(|e| format!("parse_failed: {}", e))?;

    if !json.get("telemetry").is_some_and(|v| v.is_object()) {
        if json.as_object_mut().is_some() {
            json["telemetry"] = serde_json::json!({});
        } else {
            return Err("json_top_level_not_object".to_string());
        }
    }

    if let Some(telemetry) = json.get_mut("telemetry").and_then(|v| v.as_object_mut()) {
        telemetry.insert(
            "machineId".to_string(),
            Value::String(profile.machine_id.clone()),
        );
        telemetry.insert(
            "macMachineId".to_string(),
            Value::String(profile.mac_machine_id.clone()),
        );
        telemetry.insert(
            "devDeviceId".to_string(),
            Value::String(profile.dev_device_id.clone()),
        );
        telemetry.insert("sqmId".to_string(), Value::String(profile.sqm_id.clone()));
    } else {
        return Err("telemetry_not_object".to_string());
    }

    if let Some(map) = json.as_object_mut() {
        map.insert(
            "telemetry.machineId".to_string(),
            Value::String(profile.machine_id.clone()),
        );
        map.insert(
            "telemetry.macMachineId".to_string(),
            Value::String(profile.mac_machine_id.clone()),
        );
        map.insert(
            "telemetry.devDeviceId".to_string(),
            Value::String(profile.dev_device_id.clone()),
        );
        map.insert(
            "telemetry.sqmId".to_string(),
            Value::String(profile.sqm_id.clone()),
        );
        map.insert(
            "storage.serviceMachineId".to_string(),
            Value::String(profile.dev_device_id.clone()),
        );
    }

    let updated =
        serde_json::to_string_pretty(&json).map_err(|e| format!("serialize_failed: {}", e))?;
    fs::write(storage_path, updated)
        .map_err(|e| format!("write_failed ({:?}): {}", storage_path, e))?;

    tracing::info!("device_profile_written to {:?}", storage_path);

    // Sync to SQLite for Cursor/VSCode compatibility
    if let Err(e) = sync_state_service_machine_id_value(&profile.dev_device_id) {
        tracing::warn!("sqlite_sync_failed: {}", e);
    }

    Ok(())
}

pub fn load_global_original() -> Option<DeviceProfile> {
    if let Ok(dir) = get_data_dir() {
        let path = dir.join(GLOBAL_BASELINE);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(profile) = serde_json::from_str::<DeviceProfile>(&content) {
                    return Some(profile);
                }
            }
        }
    }
    None
}

pub fn save_global_original(profile: &DeviceProfile) -> Result<(), String> {
    let dir = get_data_dir()?;
    let path = dir.join(GLOBAL_BASELINE);
    if path.exists() {
        return Ok(());
    }
    let content =
        serde_json::to_string_pretty(profile).map_err(|e| format!("serialize_failed: {}", e))?;
    fs::write(&path, content).map_err(|e| format!("write_failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_profile() {
        let profile = generate_profile();

        assert!(profile.machine_id.starts_with("auth0|user_"));
        assert_eq!(profile.machine_id.len(), 11 + 32);

        assert_eq!(profile.mac_machine_id.len(), 36);
        assert!(profile.mac_machine_id.chars().nth(14) == Some('4'));

        assert_eq!(profile.dev_device_id.len(), 36);

        assert!(profile.sqm_id.starts_with('{'));
        assert!(profile.sqm_id.ends_with('}'));
        assert_eq!(profile.sqm_id.len(), 38);
    }

    #[test]
    fn test_random_hex() {
        let hex = random_hex(32);
        assert_eq!(hex.len(), 32);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(hex.chars().all(|c| c.is_lowercase() || c.is_numeric()));
    }

    #[test]
    fn test_new_standard_machine_id() {
        let id = new_standard_machine_id();
        assert_eq!(id.len(), 36);

        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);

        assert_eq!(parts[2].chars().next(), Some('4'));

        let variant = parts[3].chars().next().unwrap();
        assert!(
            variant == '8' || variant == '9' || variant == 'a' || variant == 'b',
            "variant should be 8-b, got {}",
            variant
        );
    }

    #[test]
    fn test_profiles_are_unique() {
        let p1 = generate_profile();
        let p2 = generate_profile();

        assert_ne!(p1.machine_id, p2.machine_id);
        assert_ne!(p1.mac_machine_id, p2.mac_machine_id);
        assert_ne!(p1.dev_device_id, p2.dev_device_id);
        assert_ne!(p1.sqm_id, p2.sqm_id);
    }

    #[test]
    fn test_sync_to_state_db() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("state.vscdb");

        // Create empty DB file (sync_to_state_db checks exists())
        Connection::open(&db_path).unwrap();

        let service_id = "test-service-id-12345";
        sync_to_state_db(&db_path, service_id).unwrap();

        // Verify written value
        let conn = Connection::open(&db_path).unwrap();
        let result: String = conn
            .query_row(
                "SELECT value FROM ItemTable WHERE key = 'storage.serviceMachineId'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(result, service_id);
    }
}
