//! Per-account device fingerprint generation for anti-detection.
//!
//! Generates consistent, deterministic device profiles for each account email.
//! Each account gets its own unique fingerprint that remains stable across restarts.
//! This mimics how real Antigravity IDE instances have unique device telemetry.
//!
//! Fingerprints include:
//! - `machineId` — sha256 hex hash (mimics VSCode telemetry)
//! - `macMachineId` — UUID v4 style (network-based machine identifier)
//! - `devDeviceId` — UUID v4 (developer device identifier)
//! - `sqmId` — Uppercase UUID in braces `{UUID}` (Software Quality Metrics)
//! - `sessionId` — UUID v4 (per-session identifier, regenerated periodically)
//!
//! All IDs are deterministically derived from account email using HMAC-like
//! construction, so they remain stable but unique per account.

use std::collections::HashMap;
use std::sync::RwLock;

/// FNV-1a hash constant (same as user_agent.rs for consistency).
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

/// Secret salt for deriving fingerprints (prevents reverse-engineering).
const FINGERPRINT_SALT: &str = "antigravity-manager-fp-v1-2026";

/// Stable FNV-1a hash implementation.
fn fnv1a_hash(data: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Generate a deterministic pseudo-random byte sequence from a seed.
/// Uses a simple PRNG seeded from FNV-1a hash to generate arbitrary-length output.
fn deterministic_bytes(seed: &str, domain: &str, length: usize) -> Vec<u8> {
    let combined = format!("{}:{}:{}", FINGERPRINT_SALT, domain, seed);
    let mut state = fnv1a_hash(combined.as_bytes());
    let mut result = Vec::with_capacity(length);
    for _ in 0..length {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        result.push((state >> 33) as u8);
    }
    result
}

/// Generate a deterministic hex string of given length from seed + domain.
fn deterministic_hex(seed: &str, domain: &str, hex_len: usize) -> String {
    let bytes = deterministic_bytes(seed, domain, hex_len.div_ceil(2));
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    hex[..hex_len].to_string()
}

/// Generate a deterministic UUID v4 string from seed + domain.
fn deterministic_uuid(seed: &str, domain: &str) -> String {
    let bytes = deterministic_bytes(seed, domain, 16);
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-4{:01x}{:02x}-{:01x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6] & 0x0f, bytes[7],
        (bytes[8] & 0x03) | 0x08, bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

/// Device fingerprint profile for an account.
#[derive(Debug, Clone)]
pub struct DeviceFingerprint {
    /// SHA256-like hex hash (64 chars) — mimics VSCode `telemetry.machineId`
    pub machine_id: String,
    /// UUID-like string — mimics `telemetry.macMachineId`
    pub mac_machine_id: String,
    /// UUID v4 — mimics `telemetry.devDeviceId`
    pub dev_device_id: String,
    /// `{UUID}` uppercase — mimics `telemetry.sqmId`
    pub sqm_id: String,
    /// Session UUID (stable per account but different from device ID)
    pub session_id: String,
    /// IDE type for clientMetadata
    pub ide_type: &'static str,
    /// Platform for clientMetadata
    pub platform: &'static str,
    /// Plugin type for clientMetadata
    pub plugin_type: &'static str,
}

/// IDE types that real Antigravity instances use.
const IDE_TYPES: &[&str] = &["VSCODE", "JETBRAINS", "NEOVIM", "IDE_UNSPECIFIED", "VISUAL_STUDIO"];

/// Platform types.
const PLATFORMS: &[&str] = &["WINDOWS", "MAC", "LINUX", "PLATFORM_UNSPECIFIED"];

impl DeviceFingerprint {
    /// Generate a deterministic fingerprint for an account email.
    /// Same email always produces the same fingerprint.
    pub fn for_account(email: &str) -> Self {
        let machine_id = deterministic_hex(email, "machine_id", 64);
        let mac_machine_id = deterministic_uuid(email, "mac_machine_id");
        let dev_device_id = deterministic_uuid(email, "dev_device_id");
        let sqm_uuid = deterministic_uuid(email, "sqm_id").to_uppercase();
        let sqm_id = format!("{{{}}}", sqm_uuid);
        let session_id = deterministic_uuid(email, "session_id");

        // Deterministically pick IDE and platform based on email hash
        let type_hash = fnv1a_hash(format!("{}:ide_type", email).as_bytes());
        let ide_idx = (type_hash as usize) % IDE_TYPES.len();
        let platform_hash = fnv1a_hash(format!("{}:platform", email).as_bytes());
        let platform_idx = (platform_hash as usize) % PLATFORMS.len();

        Self {
            machine_id,
            mac_machine_id,
            dev_device_id,
            sqm_id,
            session_id,
            ide_type: IDE_TYPES[ide_idx],
            platform: PLATFORMS[platform_idx],
            plugin_type: "GEMINI",
        }
    }

    /// Build `clientMetadata` JSON value for the v1internal request body.
    pub fn client_metadata_json(&self) -> String {
        format!(
            r#"{{"ideType":"{}","platform":"{}","pluginType":"{}"}}"#,
            self.ide_type, self.platform, self.plugin_type
        )
    }

    /// Build `clientMetadata` as a `serde_json::Value`.
    pub fn client_metadata_value(&self) -> serde_json::Value {
        serde_json::json!({
            "ideType": self.ide_type,
            "platform": self.platform,
            "pluginType": self.plugin_type
        })
    }

    /// Returns a realistic `userAgent` field value for the request body.
    ///
    /// Real Cloud Code / Antigravity clients send IDE-specific user agents like:
    /// - `vscode_cloudshelleditor/0.1`
    /// - `jetbrains_cloudshelleditor/0.1`
    /// - `neovim_cloudshelleditor/0.1`
    ///
    /// This replaces the detectable `"antigravity"` string.
    pub fn body_user_agent(&self) -> &'static str {
        match self.ide_type {
            "VSCODE" => "vscode_cloudshelleditor/0.1",
            "JETBRAINS" => "jetbrains_cloudshelleditor/0.1",
            "NEOVIM" => "neovim_cloudshelleditor/0.1",
            "VISUAL_STUDIO" => "visualstudio_cloudshelleditor/0.1",
            _ => "vscode_cloudshelleditor/0.1",
        }
    }

    /// Returns a realistic request ID prefix.
    ///
    /// Real clients use prefixes matching their IDE type, not `"agent-"` or `"openai-"`.
    pub fn request_id_prefix(&self) -> &'static str {
        match self.ide_type {
            "VSCODE" => "vscode",
            "JETBRAINS" => "jetbrains",
            "NEOVIM" => "neovim",
            "VISUAL_STUDIO" => "vs",
            _ => "ide",
        }
    }

    /// Build the `x-goog-api-client` header value.
    /// Format mimics real Google Cloud SDK clients.
    pub fn api_client_header(&self) -> String {
        format!(
            "google-cloud-sdk vscode_cloudshelleditor/0.1 gcloud-node/{}",
            &self.session_id[..8]
        )
    }

    /// Build additional headers for this fingerprint.
    /// Returns a HashMap suitable for `build_headers()`'s `extra_headers` parameter.
    pub fn to_extra_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        headers.insert("x-goog-api-client".to_string(), self.api_client_header());
        headers
    }
}

/// Inject per-account fingerprint data into a v1internal request body.
///
/// This patches the JSON body to replace:
/// - `"userAgent": "antigravity"` → realistic IDE-specific value
/// - Adds `"clientMetadata"` with IDE/platform/plugin info
/// - Replaces `requestId` prefix with IDE-appropriate prefix
///
/// Call this on the final wrapped body AFTER `wrap_request()` / `transform_*()`.
pub fn inject_body_fingerprint(body: &mut serde_json::Value, email: &str) {
    let fp = get_fingerprint(email);

    // Replace userAgent with realistic IDE-specific value
    if let Some(obj) = body.as_object_mut() {
        obj.insert(
            "userAgent".to_string(),
            serde_json::Value::String(fp.body_user_agent().to_string()),
        );
        // NOTE: clientMetadata removed — Google v1internal API no longer accepts this field (returns 400)

        // Replace requestId prefix if it has a known internal prefix
        if let Some(request_id) = obj.get("requestId").and_then(|v| v.as_str()) {
            let new_id = if let Some(suffix) = request_id.strip_prefix("agent-") {
                format!("{}-{}", fp.request_id_prefix(), suffix)
            } else if let Some(suffix) = request_id.strip_prefix("openai-") {
                format!("{}-{}", fp.request_id_prefix(), suffix)
            } else if let Some(suffix) = request_id.strip_prefix("audio-") {
                format!("{}-{}", fp.request_id_prefix(), suffix)
            } else {
                request_id.to_string()
            };
            obj.insert("requestId".to_string(), serde_json::Value::String(new_id));
        }
    }
}

/// Global fingerprint cache — avoid regenerating on every request.
static FINGERPRINT_CACHE: std::sync::LazyLock<RwLock<HashMap<String, DeviceFingerprint>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// Get or create a fingerprint for the given account email.
/// Results are cached for the lifetime of the process.
pub fn get_fingerprint(email: &str) -> DeviceFingerprint {
    // Fast path: read lock
    {
        if let Ok(cache) = FINGERPRINT_CACHE.read() {
            if let Some(fp) = cache.get(email) {
                return fp.clone();
            }
        }
    }

    // Slow path: write lock, generate
    let fp = DeviceFingerprint::for_account(email);
    if let Ok(mut cache) = FINGERPRINT_CACHE.write() {
        cache.insert(email.to_string(), fp.clone());
    }
    fp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_fingerprint() {
        let fp1 = DeviceFingerprint::for_account("test@example.com");
        let fp2 = DeviceFingerprint::for_account("test@example.com");
        assert_eq!(fp1.machine_id, fp2.machine_id);
        assert_eq!(fp1.mac_machine_id, fp2.mac_machine_id);
        assert_eq!(fp1.dev_device_id, fp2.dev_device_id);
        assert_eq!(fp1.sqm_id, fp2.sqm_id);
        assert_eq!(fp1.session_id, fp2.session_id);
    }

    #[test]
    fn test_different_accounts_different_fingerprints() {
        let fp1 = DeviceFingerprint::for_account("user1@example.com");
        let fp2 = DeviceFingerprint::for_account("user2@example.com");
        assert_ne!(fp1.machine_id, fp2.machine_id);
        assert_ne!(fp1.dev_device_id, fp2.dev_device_id);
    }

    #[test]
    fn test_machine_id_length() {
        let fp = DeviceFingerprint::for_account("test@example.com");
        assert_eq!(fp.machine_id.len(), 64, "machine_id should be 64 hex chars");
    }

    #[test]
    fn test_sqm_id_format() {
        let fp = DeviceFingerprint::for_account("test@example.com");
        assert!(fp.sqm_id.starts_with('{'), "sqm_id should start with {{");
        assert!(fp.sqm_id.ends_with('}'), "sqm_id should end with }}");
        // Inner UUID should be uppercase
        let inner = &fp.sqm_id[1..fp.sqm_id.len() - 1];
        assert_eq!(inner, inner.to_uppercase(), "sqm_id inner UUID should be uppercase");
    }

    #[test]
    fn test_uuid_format() {
        let fp = DeviceFingerprint::for_account("test@example.com");
        // UUID v4 should contain '4' as version
        assert!(fp.dev_device_id.contains("-4"), "dev_device_id should be UUID v4");
        assert!(fp.mac_machine_id.contains("-4"), "mac_machine_id should be UUID v4");
    }

    #[test]
    fn test_client_metadata_json() {
        let fp = DeviceFingerprint::for_account("test@example.com");
        let json = fp.client_metadata_json();
        assert!(json.contains("ideType"));
        assert!(json.contains("platform"));
        assert!(json.contains("pluginType"));
        assert!(json.contains("GEMINI"));
    }

    #[test]
    fn test_extra_headers() {
        let fp = DeviceFingerprint::for_account("test@example.com");
        let headers = fp.to_extra_headers();
        assert!(headers.contains_key("x-goog-api-client"));
    }

    #[test]
    fn test_cache_works() {
        let fp1 = get_fingerprint("cached@example.com");
        let fp2 = get_fingerprint("cached@example.com");
        assert_eq!(fp1.machine_id, fp2.machine_id);
    }

    #[test]
    fn test_body_user_agent_not_antigravity() {
        // body_user_agent should NEVER return "antigravity"
        for email in &["a@test.com", "b@test.com", "c@test.com", "d@test.com", "e@test.com"] {
            let fp = DeviceFingerprint::for_account(email);
            let ua = fp.body_user_agent();
            assert!(!ua.contains("antigravity"), "body UA must not contain 'antigravity': {}", ua);
            assert!(ua.contains("cloudshelleditor"), "body UA should look like real IDE: {}", ua);
        }
    }

    #[test]
    fn test_request_id_prefix_not_agent() {
        for email in &["a@test.com", "b@test.com", "c@test.com", "d@test.com", "e@test.com"] {
            let fp = DeviceFingerprint::for_account(email);
            let prefix = fp.request_id_prefix();
            assert_ne!(prefix, "agent", "prefix must not be 'agent' for {}", email);
            assert_ne!(prefix, "openai", "prefix must not be 'openai' for {}", email);
        }
    }

    #[test]
    fn test_inject_body_fingerprint() {
        let mut body = serde_json::json!({
            "project": "test-project",
            "requestId": "agent-12345678-abcd",
            "userAgent": "antigravity",
            "model": "gemini-3-pro"
        });

        inject_body_fingerprint(&mut body, "test@example.com");

        // userAgent should be replaced
        let ua = body["userAgent"].as_str().unwrap();
        assert_ne!(ua, "antigravity", "userAgent must not be 'antigravity'");
        assert!(ua.contains("cloudshelleditor"), "userAgent should be IDE-style: {}", ua);

        // clientMetadata should NOT be injected (Google API rejects it)
        assert!(body.get("clientMetadata").is_none(), "clientMetadata should not be present");
    }

    #[test]
    fn test_inject_preserves_unknown_request_id() {
        let mut body = serde_json::json!({
            "requestId": "custom-prefix-12345",
            "userAgent": "antigravity"
        });

        inject_body_fingerprint(&mut body, "test@example.com");

        // Unknown prefix should be preserved as-is
        let req_id = body["requestId"].as_str().unwrap();
        assert_eq!(req_id, "custom-prefix-12345", "unknown prefix should be preserved");
    }
}
