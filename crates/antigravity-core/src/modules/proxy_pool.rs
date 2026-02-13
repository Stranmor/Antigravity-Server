//! Per-account proxy pool auto-assignment.
//!
//! When `account_proxy_pool.enabled` is true in config, new accounts
//! without an explicit proxy_url are automatically assigned one from the pool.

use antigravity_types::models::config::{AccountProxyPoolConfig, ProxyAssignmentStrategy};

use crate::modules::account;

/// Pick a proxy URL from the pool for a new account.
///
/// Returns `None` if the pool is disabled, empty, or the account already has a proxy.
pub fn assign_proxy_from_pool(
    pool: &AccountProxyPoolConfig,
    existing_proxy: Option<&str>,
) -> Option<String> {
    // Don't override explicit proxy
    if existing_proxy.is_some() {
        return None;
    }

    if !pool.enabled || pool.urls.is_empty() {
        return None;
    }

    let url = match pool.strategy {
        ProxyAssignmentStrategy::RoundRobin | ProxyAssignmentStrategy::LeastUsed => {
            // Count how many existing accounts use each proxy
            pick_least_used(pool)
        },
        ProxyAssignmentStrategy::Random => {
            use rand::Rng;
            let idx = rand::thread_rng().gen_range(0..pool.urls.len());
            pool.urls[idx].clone()
        },
    };

    Some(url)
}

/// Pick the proxy URL that is used by the fewest existing accounts.
fn pick_least_used(pool: &AccountProxyPoolConfig) -> String {
    // Try to load accounts and count proxy usage
    let usage_counts = match account::list_accounts() {
        Ok(accounts) => {
            let mut counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            // Initialize all pool URLs with 0
            for url in &pool.urls {
                counts.insert(url.as_str(), 0);
            }
            // Count existing assignments
            for acc in &accounts {
                if let Some(ref purl) = acc.proxy_url {
                    if let Some(count) = counts.get_mut(purl.as_str()) {
                        *count += 1;
                    }
                }
            }
            counts.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<Vec<_>>()
        },
        Err(_) => {
            // Fallback to first proxy if we can't read accounts
            return pool.urls[0].clone();
        },
    };

    // Pick the one with the lowest count (ties broken by pool order)
    usage_counts
        .into_iter()
        .min_by_key(|(_, count)| *count)
        .map(|(url, _)| url)
        .unwrap_or_else(|| pool.urls[0].clone())
}
