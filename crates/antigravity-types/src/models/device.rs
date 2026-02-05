//! Device fingerprint types for account isolation.
//!
//! Provides device profile structures for fingerprint protection.
//! Each account can have a unique device fingerprint to prevent
//! cross-account correlation by upstream APIs.

use serde::{Deserialize, Serialize};

/// Device fingerprint profile matching Cursor/VSCode storage format.
///
/// These fields are written to `storage.json` and used by the upstream
/// API to identify the "device" making requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceProfile {
    /// Machine ID in auth0 format: `auth0|user_{random_hex_32}`
    pub machine_id: String,

    /// MAC-based machine ID in UUID v4 format
    pub mac_machine_id: String,

    /// Cursor/VSCode device ID (UUID v4)
    pub dev_device_id: String,

    /// SQM telemetry ID in format `{UUID}` (with braces, uppercase)
    pub sqm_id: String,
}

/// Historical version of a device profile for rollback support.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeviceProfileVersion {
    /// Unique version ID
    pub id: String,

    /// The device profile snapshot
    pub profile: DeviceProfile,

    /// Optional label (e.g., "generated", "captured", "baseline")
    pub label: Option<String>,

    /// Whether this is the currently active profile
    pub is_current: bool,

    /// ISO 8601 timestamp of when this version was created
    pub created_at: String,
}

/// Collection of device profiles for an account.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeviceProfiles {
    /// Currently bound device profile (if any)
    pub bound_profile: Option<DeviceProfile>,

    /// Historical versions for rollback
    pub history: Vec<DeviceProfileVersion>,

    /// Global baseline profile (shared original)
    pub baseline: Option<DeviceProfile>,

    /// Current profile read from storage.json
    pub current_storage: Option<DeviceProfile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_profile_serialization() {
        let profile = DeviceProfile {
            machine_id: "auth0|user_abc123".to_string(),
            mac_machine_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            dev_device_id: "660e8400-e29b-41d4-a716-446655440001".to_string(),
            sqm_id: "{770E8400-E29B-41D4-A716-446655440002}".to_string(),
        };

        let json = serde_json::to_string(&profile).unwrap();
        let parsed: DeviceProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, parsed);
    }

    #[test]
    fn test_device_profile_version() {
        let version = DeviceProfileVersion {
            id: "v1".to_string(),
            profile: DeviceProfile {
                machine_id: "auth0|user_test".to_string(),
                mac_machine_id: "test-uuid".to_string(),
                dev_device_id: "dev-uuid".to_string(),
                sqm_id: "{SQM-UUID}".to_string(),
            },
            label: Some("generated".to_string()),
            is_current: true,
            created_at: "2026-02-01T12:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&version).unwrap();
        assert!(json.contains("generated"));
        assert!(json.contains("is_current"));
    }
}
