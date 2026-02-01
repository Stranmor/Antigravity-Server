//! User-Agent rotation for fingerprint protection.
//!
//! Provides a pool of realistic browser User-Agent strings with deterministic
//! selection based on account email hash. Each account gets a consistent UA
//! to avoid fingerprint inconsistency within the same session.

/// FNV-1a hash constant (stable across Rust versions, unlike DefaultHasher).
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Stable FNV-1a hash implementation.
fn fnv1a_hash(data: &str) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Realistic browser User-Agent strings (2024-2025 versions).
/// Mix of Chrome, Firefox, Edge on Windows, macOS, Linux.
const USER_AGENT_POOL: &[&str] = &[
    // Chrome on Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36",
    // Chrome on macOS
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    // Chrome on Linux
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
    // Firefox on Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0",
    // Firefox on macOS
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:133.0) Gecko/20100101 Firefox/133.0",
    // Firefox on Linux
    "Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0",
    // Edge on Windows
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0",
    // Safari on macOS
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Safari/605.1.15",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15",
];

/// Fallback UA when no account context is available.
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
    fn test_different_emails_different_uas() {
        // Pre-verified emails that hash to different indices with FNV-1a
        let ua1 = get_user_agent_for_account("user1@example.com");
        let ua2 = get_user_agent_for_account("user2@example.com");
        assert_ne!(ua1, ua2, "Different emails should get different UAs");
    }

    #[test]
    fn test_all_uas_are_realistic() {
        for ua in USER_AGENT_POOL {
            assert!(ua.contains("Mozilla/5.0"), "UA should start with Mozilla");
            assert!(
                ua.contains("Windows") || ua.contains("Macintosh") || ua.contains("Linux"),
                "UA should contain OS identifier"
            );
        }
    }

    #[test]
    fn test_pool_size() {
        assert!(
            USER_AGENT_POOL.len() >= 10,
            "Pool should have at least 10 UAs for diversity"
        );
    }
}
