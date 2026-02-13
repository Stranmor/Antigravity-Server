//! User-Agent rotation for fingerprint protection.
//!
//! Provides a pool of application-style User-Agent strings that Google's
//! Cloud Code API accepts. Uses deterministic selection based on account
//! email hash to ensure consistency within sessions.
//!
//! **CRITICAL**: Browser-style UAs (Mozilla/5.0...) are REJECTED by Google
//! with `CONSUMER_INVALID` errors. Only application-style UAs are accepted.
//!
//! The pool now includes:
//! - Multiple Antigravity client versions (simulating diverse install base)
//! - `google-api-nodejs-client` UA (used by CLIProxyAPI and Google's own SDKs)
//! - Per-account deterministic selection for fingerprint consistency

/// FNV-1a hash constant (stable across Rust versions, unlike DefaultHasher).
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

/// Stable FNV-1a hash implementation.
fn fnv1a_hash(data: &str) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Application-style User-Agent strings accepted by Google Cloud Code API.
///
/// **Format**: `antigravity/VERSION PLATFORM/ARCH`
///
/// Pool covers multiple versions to simulate a diverse install base.
/// All versions correspond to real Antigravity releases to avoid detection.
///
/// **DO NOT use browser-style UAs** (Mozilla/5.0...) - they cause
/// `CONSUMER_INVALID` errors from Google's API validation.
const USER_AGENT_POOL: &[&str] = &[
    // === Latest stable (1.104.x) - majority of users ===
    "antigravity/1.104.0 darwin/arm64",
    "antigravity/1.104.0 darwin/amd64",
    "antigravity/1.104.0 linux/amd64",
    "antigravity/1.104.0 linux/arm64",
    "antigravity/1.104.0 windows/amd64",
    "antigravity/1.104.0 windows/arm64",
    // === Previous stable (1.102.x) - users on slightly older version ===
    "antigravity/1.102.1 darwin/arm64",
    "antigravity/1.102.1 darwin/amd64",
    "antigravity/1.102.1 linux/amd64",
    "antigravity/1.102.1 windows/amd64",
    // === Older stable (1.100.x) - some users still on this ===
    "antigravity/1.100.0 darwin/arm64",
    "antigravity/1.100.0 linux/amd64",
    "antigravity/1.100.0 windows/amd64",
    // === Google API client UA (used by SDK/CLIProxyAPI) ===
    "google-api-nodejs-client/9.15.1",
    "google-api-nodejs-client/9.14.0",
];

/// Default UA when no account context is available.
/// Uses the most common combination (macOS ARM64, latest version).
pub const DEFAULT_USER_AGENT: &str = "antigravity/1.104.0 darwin/arm64";

/// Get a deterministic User-Agent for a given account email.
///
/// Uses FNV-1a hash of email to select from pool. Same email always gets same UA,
/// ensuring fingerprint consistency within account sessions.
#[inline]
pub fn get_user_agent_for_account(email: &str) -> &'static str {
    let hash = fnv1a_hash(email);
    let index = (hash as usize) % USER_AGENT_POOL.len();
    USER_AGENT_POOL[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_selection() {
        let email = "test@example.com";
        let ua1 = get_user_agent_for_account(email);
        let ua2 = get_user_agent_for_account(email);
        assert_eq!(ua1, ua2, "Same email should always get same UA");
    }

    #[test]
    fn test_different_emails_distribution() {
        let emails = [
            "user1@example.com",
            "user2@example.com",
            "user3@example.com",
            "user4@example.com",
            "user5@example.com",
            "user6@example.com",
            "user7@example.com",
            "user8@example.com",
        ];
        let uas: Vec<_> = emails.iter().map(|e| get_user_agent_for_account(e)).collect();
        let unique_count = uas.iter().collect::<std::collections::HashSet<_>>().len();
        assert!(unique_count >= 2, "Should have at least 2 unique UAs from 8 emails");
    }

    #[test]
    fn test_all_uas_are_application_style() {
        for ua in USER_AGENT_POOL {
            assert!(
                ua.starts_with("antigravity/") || ua.starts_with("google-api-nodejs-client/"),
                "UA '{}' should be application-style",
                ua
            );
            assert!(!ua.contains("Mozilla"), "UA must NOT contain Mozilla (browser-style)");
        }
    }

    #[test]
    fn test_pool_size() {
        assert!(
            USER_AGENT_POOL.len() >= 15,
            "Pool should have at least 15 UAs, got {}",
            USER_AGENT_POOL.len()
        );
    }

    #[test]
    fn test_default_ua() {
        assert_eq!(DEFAULT_USER_AGENT, "antigravity/1.104.0 darwin/arm64");
    }

    #[test]
    fn test_no_old_version() {
        // Ensure we no longer use the detectable 4.0.8 version
        for ua in USER_AGENT_POOL {
            assert!(!ua.contains("4.0.8"), "UA pool should not contain old 4.0.8 version: {}", ua);
        }
    }
}
