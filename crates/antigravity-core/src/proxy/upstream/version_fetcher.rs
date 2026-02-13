//! Dynamic Antigravity version fetcher.
//!
//! Fetches the current Antigravity IDE version at startup from Google's
//! auto-updater API, with fallbacks to the changelog page and a hardcoded
//! default. The fetched version is used to construct realistic User-Agent
//! strings for fingerprint protection.
//!
//! Uses `std::thread::spawn` to avoid blocking the async runtime during
//! initialization (same pattern as upstream `constants.rs`).

use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

/// Hardcoded fallback version when all fetch attempts fail.
/// This is the Antigravity IDE version, NOT our fork's Cargo version.
const FALLBACK_VERSION: &str = "1.104.0";

/// Auto-updater API endpoint (primary source).
const UPDATER_API_URL: &str = "https://antigravity-auto-updater-974169037036.us-central1.run.app";

/// Changelog page (secondary source).
const CHANGELOG_URL: &str = "https://antigravity.google/changelog";

/// Maximum bytes to scan from the changelog page.
const CHANGELOG_SCAN_LIMIT: usize = 5000;

/// Maximum response body size (64 KB) to prevent memory pressure from malicious upstream.
const MAX_RESPONSE_BYTES: usize = 65_536;

const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Lazily fetched current Antigravity version.
///
/// Initialization happens on first access via a background `std::thread`
/// that creates its own tokio runtime to run async HTTP requests.
/// This avoids blocking the main async runtime.
static CURRENT_VERSION: LazyLock<String> = LazyLock::new(|| {
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let version = fetch_version_sync();
        // Channel send failure means receiver was dropped — harmless
        let _ = tx.send(version);
    });

    // Wait for the spawned thread; timeout after 12s total
    // (two 5s HTTP requests + some overhead)
    match rx.recv_timeout(Duration::from_secs(12)) {
        Ok(v) => v,
        Err(_) => {
            tracing::warn!(
                version = FALLBACK_VERSION,
                "Version fetch thread timed out, using fallback"
            );
            FALLBACK_VERSION.to_string()
        },
    }
});

/// Get the current Antigravity version string (e.g., `"1.115.0"`).
///
/// First access triggers a background fetch. Subsequent calls return
/// the cached value with zero overhead.
pub fn get_current_version() -> &'static str {
    &CURRENT_VERSION
}

/// Parse a semver-style version (`X.Y.Z`) from text using regex.
/// Rejects IP-address-like matches (e.g. `192.168.1.1`) by checking
/// that the match is not followed by another dot-digit segment.
pub fn parse_version(text: &str) -> Option<String> {
    let re = Regex::new(r"\d+\.\d+\.\d+").ok()?;
    for m in re.find_iter(text) {
        let after = &text[m.end()..];
        // Reject if followed by `.\d` (IP-like: 192.168.1.1)
        if after.starts_with('.')
            && after
                .get(1..2)
                .is_some_and(|c| c.as_bytes().first().is_some_and(|b| b.is_ascii_digit()))
        {
            continue;
        }
        return Some(m.as_str().to_string());
    }
    None
}

/// Synchronous version fetch with ordered fallbacks.
/// Runs in a dedicated thread with its own tokio runtime.
fn fetch_version_sync() -> String {
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "Failed to build tokio runtime for version fetch");
            return FALLBACK_VERSION.to_string();
        },
    };

    rt.block_on(async {
        // Attempt 1: Auto-updater API
        if let Some(v) = fetch_from_updater_api().await {
            tracing::info!(version = %v, source = "updater-api", "Fetched Antigravity version");
            return v;
        }

        // Attempt 2: Changelog page
        if let Some(v) = fetch_from_changelog().await {
            tracing::info!(version = %v, source = "changelog", "Fetched Antigravity version");
            return v;
        }

        // Attempt 3: Hardcoded fallback
        tracing::info!(
            version = FALLBACK_VERSION,
            source = "fallback",
            "Using hardcoded Antigravity version"
        );
        FALLBACK_VERSION.to_string()
    })
}

/// Read response body capped at [`MAX_RESPONSE_BYTES`] to prevent memory pressure.
async fn read_response_capped(resp: reqwest::Response) -> Option<Vec<u8>> {
    // Check Content-Length first for early rejection
    if let Some(len) = resp.content_length() {
        if len > MAX_RESPONSE_BYTES as u64 {
            tracing::debug!(
                content_length = len,
                max = MAX_RESPONSE_BYTES,
                "Response body exceeds size limit, skipping"
            );
            return None;
        }
    }
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        tracing::debug!(
            actual = bytes.len(),
            max = MAX_RESPONSE_BYTES,
            "Response body exceeds size limit after download"
        );
        return None;
    }
    Some(bytes.to_vec())
}

/// Fetch version from the auto-updater API.
async fn fetch_from_updater_api() -> Option<String> {
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build().ok()?;

    let resp = client.get(UPDATER_API_URL).send().await.ok()?;

    if !resp.status().is_success() {
        tracing::debug!(
            status = %resp.status(),
            "Updater API returned non-success status"
        );
        return None;
    }

    let bytes = read_response_capped(resp).await?;
    let body = String::from_utf8_lossy(&bytes);
    parse_version(&body)
}

/// Fetch version from the changelog page (scan first N chars).
async fn fetch_from_changelog() -> Option<String> {
    let client = reqwest::Client::builder().timeout(FETCH_TIMEOUT).build().ok()?;

    let resp = client.get(CHANGELOG_URL).send().await.ok()?;

    if !resp.status().is_success() {
        tracing::debug!(
            status = %resp.status(),
            "Changelog page returned non-success status"
        );
        return None;
    }

    let bytes = read_response_capped(resp).await?;
    let body = String::from_utf8_lossy(&bytes);
    // Use .get() to avoid panic on multi-byte UTF-8 boundary
    let scan_window = body.get(..CHANGELOG_SCAN_LIMIT).unwrap_or(&body);
    parse_version(scan_window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_valid_semver() {
        assert_eq!(parse_version("1.115.0"), Some("1.115.0".to_string()));
        assert_eq!(parse_version("2.0.1"), Some("2.0.1".to_string()));
        assert_eq!(parse_version("0.1.0"), Some("0.1.0".to_string()));
    }

    #[test]
    fn test_parse_version_embedded_in_text() {
        assert_eq!(
            parse_version("antigravity version 1.115.0 released"),
            Some("1.115.0".to_string())
        );
        assert_eq!(parse_version("{\"version\":\"1.120.3\"}"), Some("1.120.3".to_string()));
    }

    #[test]
    fn test_parse_version_with_suffix() {
        // Regex captures digits only — suffix is ignored
        assert_eq!(parse_version("1.115.0-beta"), Some("1.115.0".to_string()));
    }

    #[test]
    fn test_parse_version_rejects_ip_addresses() {
        assert_eq!(parse_version("192.168.1.1"), None);
        assert_eq!(parse_version("10.0.0.1"), None);
        assert_eq!(parse_version("connected to 172.16.0.1 on port 80"), None);
    }

    #[test]
    fn test_parse_version_invalid() {
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("abc"), None);
    }

    #[test]
    fn test_parse_version_multiple_takes_first() {
        assert_eq!(parse_version("1.100.0 and 1.115.0"), Some("1.100.0".to_string()));
    }

    #[test]
    fn test_fallback_version_is_valid() {
        assert!(parse_version(FALLBACK_VERSION).is_some());
    }

    #[test]
    fn test_get_current_version_returns_nonempty() {
        let version = get_current_version();
        assert!(!version.is_empty());
        // Must be parseable as semver
        assert!(
            parse_version(version).is_some(),
            "get_current_version() returned unparseable: {}",
            version
        );
    }
}
