//! Device fingerprint generation for account isolation.
//!
//! Generates unique device fingerprints (Cursor/VSCode style) to prevent
//! cross-account correlation by upstream APIs.

use antigravity_types::models::DeviceProfile;
use rand::Rng;
use uuid::Uuid;

/// Generate a new set of device fingerprints (Cursor/VSCode style).
///
/// Each field mimics the format used by real Cursor/VSCode installations:
/// - `machine_id`: auth0 format `auth0|user_{random_hex_32}`
/// - `mac_machine_id`: UUID v4 with special variant bits
/// - `dev_device_id`: Standard UUID v4
/// - `sqm_id`: Uppercase UUID with braces `{UUID}`
pub fn generate_profile() -> DeviceProfile {
    DeviceProfile {
        machine_id: format!("auth0|user_{}", random_hex(32)),
        mac_machine_id: new_standard_machine_id(),
        dev_device_id: Uuid::new_v4().to_string(),
        sqm_id: format!("{{{}}}", Uuid::new_v4().to_string().to_uppercase()),
    }
}

/// Generate random lowercase hexadecimal string (0-9, a-f) of given length.
fn random_hex(length: usize) -> String {
    const HEX_CHARS: &[u8] = b"0123456789abcdef";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| HEX_CHARS[rng.gen_range(0..16)] as char)
        .collect()
}

/// Generate UUID v4 with specific variant bits (8-b in position 19).
/// Format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx where y is 8, 9, a, or b.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_profile() {
        let profile = generate_profile();

        // machine_id format
        assert!(profile.machine_id.starts_with("auth0|user_"));
        assert_eq!(profile.machine_id.len(), 11 + 32); // "auth0|user_" + 32 hex

        // mac_machine_id is UUID format
        assert_eq!(profile.mac_machine_id.len(), 36);
        assert!(profile.mac_machine_id.chars().nth(14) == Some('4'));

        // dev_device_id is UUID format
        assert_eq!(profile.dev_device_id.len(), 36);

        // sqm_id has braces and uppercase
        assert!(profile.sqm_id.starts_with('{'));
        assert!(profile.sqm_id.ends_with('}'));
        assert_eq!(profile.sqm_id.len(), 38); // {UUID}
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

        // Check structure: 8-4-4-4-12
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);

        // Check version 4 marker
        assert_eq!(parts[2].chars().next(), Some('4'));

        // Check variant bits (8, 9, a, b)
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
}
