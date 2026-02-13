//! User-Agent rotation for fingerprint protection.
//!
//! Provides a pool of application-style User-Agent strings that Google's
//! Cloud Code API accepts. Uses deterministic selection based on account
//! email hash to ensure consistency within sessions.
//!
//! **CRITICAL**: Browser-style UAs (Mozilla/5.0...) are REJECTED by Google
//! with `CONSUMER_INVALID` errors. Only application-style UAs are accepted.
//!
//! The pool is built dynamically from the fetched Antigravity version:
//! - Latest version (fetched) × 6 platforms
//! - Previous version (minor - 2) × 4 platforms
//! - Older version (minor - 4) × 3 platforms
//! - `google-api-nodejs-client` entries (SDK/CLIProxyAPI)

use super::version_fetcher;
use std::sync::LazyLock;

/// FNV-1a hash constant (stable across Rust versions, unlike DefaultHasher).
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

/// Platforms for latest version (all 6).
const LATEST_PLATFORMS: &[&str] = &[
    "darwin/arm64",
    "darwin/amd64",
    "linux/amd64",
    "linux/arm64",
    "windows/amd64",
    "windows/arm64",
];

/// Platforms for previous version (main 4).
const PREVIOUS_PLATFORMS: &[&str] =
    &["darwin/arm64", "darwin/amd64", "linux/amd64", "windows/amd64"];

/// Platforms for older version (common 3).
const OLDER_PLATFORMS: &[&str] = &["darwin/arm64", "linux/amd64", "windows/amd64"];

/// Stable FNV-1a hash implementation.
fn fnv1a_hash(data: &str) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Derive a "previous" version by decrementing the minor component.
///
/// Given `"1.115.0"` and `decrement=2`, returns `"1.113.0"`.
/// If minor would go below 0, clamps to 0.
fn derive_previous_version(version: &str, decrement: u32) -> String {
    let parts: Vec<&str> = version.splitn(3, '.').collect();
    if parts.len() < 3 {
        return version.to_string();
    }
    let major = parts[0];
    let minor: u32 = parts[1].parse().unwrap_or(0);
    let patch = parts[2];
    let new_minor = minor.saturating_sub(decrement);
    format!("{}.{}.{}", major, new_minor, patch)
}

/// Dynamically built User-Agent pool from fetched Antigravity version.
///
/// Pool structure: 6 + 4 + 3 + 2 = 15 entries minimum.
static USER_AGENT_POOL: LazyLock<Vec<String>> = LazyLock::new(|| {
    let latest = version_fetcher::get_current_version();
    let previous = derive_previous_version(latest, 2);
    let older = derive_previous_version(latest, 4);

    let mut pool = Vec::with_capacity(15);

    for platform in LATEST_PLATFORMS {
        pool.push(format!("antigravity/{} {}", latest, platform));
    }
    for platform in PREVIOUS_PLATFORMS {
        pool.push(format!("antigravity/{} {}", previous, platform));
    }
    for platform in OLDER_PLATFORMS {
        pool.push(format!("antigravity/{} {}", older, platform));
    }

    pool.push("google-api-nodejs-client/9.15.1".to_string());
    pool.push("google-api-nodejs-client/9.14.0".to_string());

    pool
});

/// Default UA when no account context is available.
/// Uses the most common combination (macOS ARM64, latest version).
pub fn default_user_agent() -> &'static str {
    USER_AGENT_POOL.first().map(|s| s.as_str()).unwrap_or("antigravity/1.104.0 darwin/arm64")
}

/// Get a deterministic User-Agent for a given account email.
///
/// Uses FNV-1a hash of email to select from pool. Same email always gets same UA,
/// ensuring fingerprint consistency within account sessions.
#[inline]
pub fn get_user_agent_for_account(email: &str) -> &'static str {
    let pool = &*USER_AGENT_POOL;
    let hash = fnv1a_hash(email);
    let index = (hash as usize) % pool.len();
    pool[index].as_str()
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
        for ua in USER_AGENT_POOL.iter() {
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
        let ua = default_user_agent();
        assert!(ua.starts_with("antigravity/"), "Default UA should start with antigravity/");
        assert!(ua.contains("darwin/arm64"), "Default UA should be darwin/arm64");
    }

    #[test]
    fn test_no_old_version() {
        for ua in USER_AGENT_POOL.iter() {
            assert!(!ua.contains("4.0.8"), "UA pool should not contain old 4.0.8 version: {}", ua);
        }
    }

    #[test]
    fn test_derive_previous_version() {
        assert_eq!(derive_previous_version("1.115.0", 2), "1.113.0");
        assert_eq!(derive_previous_version("1.115.0", 4), "1.111.0");
        assert_eq!(derive_previous_version("1.1.0", 4), "1.0.0");
        assert_eq!(derive_previous_version("2.0.0", 2), "2.0.0");
    }

    #[test]
    fn test_fetched_version_in_pool() {
        let version = version_fetcher::get_current_version();
        let has_version = USER_AGENT_POOL.iter().any(|ua| ua.contains(version));
        assert!(has_version, "Pool should contain fetched version {}", version);
    }

    #[test]
    fn test_pool_has_derived_versions() {
        let version = version_fetcher::get_current_version();
        let prev = derive_previous_version(version, 2);
        let older = derive_previous_version(version, 4);

        let has_prev = USER_AGENT_POOL.iter().any(|ua| ua.contains(&prev));
        let has_older = USER_AGENT_POOL.iter().any(|ua| ua.contains(&older));

        assert!(has_prev, "Pool should contain previous version {}", prev);
        assert!(has_older, "Pool should contain older version {}", older);
    }
}
