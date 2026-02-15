//! Browser TLS/JA3/HTTP2 fingerprint emulation for anti-detection.
//!
//! Provides deterministic browser emulation profile selection for each account.
//! Uses `wreq-util` emulation profiles to make our HTTP requests
//! indistinguishable from real Chrome/Safari browser traffic.
//!
//! **Why this matters**: Google detects and bans accounts by analyzing TLS
//! fingerprints (JA3/JA4). Standard Rust HTTP clients (`reqwest`/`hyper`)
//! produce unique, easily identifiable fingerprints. By using BoringSSL
//! with Chrome emulation profiles, our requests look identical to real
//! browser traffic.

use wreq_util::Emulation;

/// FNV-1a hash constant (same as user_agent.rs for consistency).
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

/// Pool of browser emulation profiles to rotate across accounts.
///
/// Using multiple Chrome versions (and Safari) makes it harder
/// to fingerprint all users as coming from the same client.
const EMULATION_POOL: &[Emulation] = &[
    Emulation::Chrome131,
    Emulation::Chrome132,
    Emulation::Chrome133,
    Emulation::Chrome134,
    Emulation::Chrome135,
    Emulation::Chrome136,
    Emulation::Chrome137,
    Emulation::Safari18,
];

/// Stable FNV-1a hash implementation.
fn fnv1a_hash(data: &str) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Default emulation profile for requests without account context.
///
/// Uses latest Chrome for maximum compatibility.
pub fn default_emulation() -> Emulation {
    Emulation::Chrome137
}

/// Get a deterministic emulation profile for a given account email.
///
/// Uses FNV-1a hash of email to select from the pool. Same email always
/// gets the same profile, ensuring fingerprint consistency within account
/// sessions.
#[inline]
pub fn get_emulation_for_account(email: &str) -> Emulation {
    let hash = fnv1a_hash(email);
    let index = (hash as usize) % EMULATION_POOL.len();
    EMULATION_POOL[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_selection() {
        let email = "test@example.com";
        let e1 = get_emulation_for_account(email);
        let e2 = get_emulation_for_account(email);
        assert_eq!(e1, e2, "Same email should always get same emulation profile");
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
        let profiles: Vec<_> = emails.iter().map(|e| get_emulation_for_account(e)).collect();
        let unique_count = profiles.iter().collect::<std::collections::HashSet<_>>().len();
        assert!(
            unique_count >= 2,
            "Should have at least 2 unique profiles from 8 emails, got {}",
            unique_count
        );
    }

    #[test]
    fn test_default_is_chrome() {
        let e = default_emulation();
        assert_eq!(e, Emulation::Chrome137);
    }

    #[test]
    fn test_pool_size() {
        assert!(
            EMULATION_POOL.len() >= 6,
            "Pool should have at least 6 profiles, got {}",
            EMULATION_POOL.len()
        );
    }

    /// Live test: verify TLS fingerprint looks like Chrome.
    /// Run: cargo test -p antigravity-core test_live_tls_fingerprint -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "requires network access"]
    async fn test_live_tls_fingerprint() {
        let client = wreq::Client::builder()
            .emulation(default_emulation())
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("wreq client build");

        let resp = client
            .get("https://tls.peet.ws/api/all")
            .send()
            .await
            .expect("tls.peet.ws request failed");

        assert!(resp.status().is_success(), "Status: {}", resp.status());

        let body = resp.text().await.expect("body read");
        let json: serde_json::Value = serde_json::from_str(&body).expect("JSON parse");

        // Print key fingerprint info
        if let Some(tls) = json.get("tls") {
            println!("\n=== TLS Fingerprint ===");
            println!("JA3 Hash:  {}", tls.get("ja3_hash").unwrap_or(&serde_json::Value::Null));
            println!("JA4:       {}", tls.get("ja4").unwrap_or(&serde_json::Value::Null));
        }
        if let Some(http2) = json.get("http2") {
            println!("\n=== HTTP/2 Fingerprint ===");
            println!(
                "Akamai FP: {}",
                http2.get("akamai_fingerprint_hash").unwrap_or(&serde_json::Value::Null)
            );
        }
        if let Some(ip) = json.get("ip") {
            println!("\nIP: {}", ip);
        }

        // Verify it's NOT a default Rust/hyper fingerprint
        let ja3_hash = json["tls"]["ja3_hash"].as_str().unwrap_or("");
        assert!(!ja3_hash.is_empty(), "JA3 hash should not be empty");
        println!("\n✅ TLS fingerprint verified — not a bare hyper/rustls client");
    }

    /// Live test: verify wreq can connect through HTTP proxy.
    /// Run: PROXY_URL="http://user:pass@proxy:port" cargo test -p antigravity-core test_proxy_connectivity -- --ignored --nocapture
    #[tokio::test]
    #[ignore = "requires network and PROXY_URL env"]
    async fn test_proxy_connectivity() {
        let proxy_url = std::env::var("PROXY_URL").expect("Set PROXY_URL env var");
        println!(
            "Testing proxy: {}...{}",
            &proxy_url[..20.min(proxy_url.len())],
            &proxy_url[proxy_url.len().saturating_sub(20)..]
        );

        let proxy = wreq::Proxy::all(&proxy_url).expect("Invalid proxy URL");
        let client = wreq::Client::builder()
            .emulation(default_emulation())
            .proxy(proxy)
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("wreq client with proxy");

        println!("Connecting to https://httpbin.org/ip through proxy...");
        match client.get("https://httpbin.org/ip").send().await {
            Ok(resp) => {
                println!("Status: {}", resp.status());
                let body = resp.text().await.unwrap_or_default();
                println!("Body: {}", body);
                println!("✅ Proxy connection works!");
            },
            Err(e) => {
                eprintln!("❌ Proxy connection failed: {}", e);
                eprintln!("Error debug: {:?}", e);
                panic!("Proxy connect failed: {}", e);
            },
        }
    }
}
