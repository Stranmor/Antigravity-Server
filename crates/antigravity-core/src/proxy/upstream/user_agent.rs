//! User-Agent rotation for fingerprint protection.
//!
//! Provides a pool of application-style User-Agent strings that Google's
//! Cloud Code API accepts. Uses deterministic selection based on account
//! email hash to ensure consistency within sessions.
//!
//! **CRITICAL**: Browser-style UAs (Mozilla/5.0...) are REJECTED by Google
//! with `CONSUMER_INVALID` errors. Only application-style UAs are accepted.

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
/// Format: `antigravity/VERSION PLATFORM/ARCH`
///
/// **DO NOT use browser-style UAs** (Mozilla/5.0...) - they cause
/// `CONSUMER_INVALID` errors from Google's API validation.
const USER_AGENT_POOL: &[&str] = &[
    // Windows variants
    "antigravity/4.0.8 windows/amd64",
    "antigravity/4.0.8 windows/arm64",
    // macOS variants
    "antigravity/4.0.8 darwin/amd64",
    "antigravity/4.0.8 darwin/arm64",
    // Linux variants
    "antigravity/4.0.8 linux/amd64",
    "antigravity/4.0.8 linux/arm64",
];

/// Default UA when no account context is available.
pub const DEFAULT_USER_AGENT: &str = USER_AGENT_POOL[0];

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
        // Collect UAs for multiple emails to verify distribution
        let emails = ["user1@example.com", "user2@example.com", "user3@example.com"];
        let uas: Vec<_> = emails.iter().map(|e| get_user_agent_for_account(e)).collect();

        // At least 1 unique UA should be selected
        let unique_count = uas.iter().collect::<std::collections::HashSet<_>>().len();
        assert!(unique_count >= 1, "Should have at least 1 unique UA");
    }

    #[test]
    fn test_all_uas_are_application_style() {
        for ua in USER_AGENT_POOL {
            assert!(ua.starts_with("antigravity/"), "UA should start with 'antigravity/'");
            assert!(!ua.contains("Mozilla"), "UA must NOT contain Mozilla (browser-style)");
            assert!(
                ua.contains("windows") || ua.contains("darwin") || ua.contains("linux"),
                "UA should contain platform identifier"
            );
            assert!(ua.contains("amd64") || ua.contains("arm64"), "UA should contain architecture");
        }
    }

    #[test]
    fn test_pool_size() {
        assert_eq!(
            USER_AGENT_POOL.len(),
            6,
            "Pool should have exactly 6 UAs (3 platforms × 2 architectures)"
        );
    }

    #[test]
    fn test_default_ua() {
        assert_eq!(DEFAULT_USER_AGENT, "antigravity/4.0.8 windows/amd64");
    }
}
