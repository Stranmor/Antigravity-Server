//! Proxy pool with rotation for ban protection.
//!
//! Supports multiple proxy servers with configurable rotation strategies:
//! - **RoundRobin**: Evenly distributes requests across all proxies
//! - **Random**: Randomly selects a proxy for each request
//! - **PerAccount**: Deterministically binds each account to a specific proxy (sticky)
//!
//! Each proxy gets its own cached `reqwest::Client` for connection reuse.

use antigravity_types::models::{ProxyRotationStrategy, UpstreamProxyConfig, UpstreamProxyMode};
use reqwest::Client;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::RwLock;
use tokio::time::Duration;

use super::upstream::user_agent::default_user_agent;

/// Parse a proxy URL string into a normalized URL.
///
/// Supports:
/// - Standard format: `http://host:port`, `socks5://host:port`, `http://user:pass@host:port`
/// - Webshare format: `ip:port:user:pass` → auto-converts to `http://user:pass@ip:port`
pub fn parse_proxy_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Empty proxy URL".to_string());
    }

    // Already has a scheme — validate as URL
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("socks5://")
        || trimmed.starts_with("socks5h://")
    {
        // Validate it parses
        url::Url::parse(trimmed).map_err(|e| format!("Invalid proxy URL '{}': {}", trimmed, e))?;
        return Ok(trimmed.to_string());
    }

    // Try Webshare format: ip:port:user:pass
    let parts: Vec<&str> = trimmed.splitn(4, ':').collect();
    if parts.len() == 4 {
        let ip = parts[0];
        let port = parts[1];
        let user = parts[2];
        let pass = parts[3];

        // Validate port is numeric
        port.parse::<u16>()
            .map_err(|_| format!("Invalid port '{}' in proxy '{}'", port, trimmed))?;

        let url = format!("http://{}:{}@{}:{}", user, pass, ip, port);
        tracing::debug!(raw = %trimmed, parsed = %url, "Parsed Webshare proxy format");
        return Ok(url);
    }

    // Try ip:port (no auth)
    if parts.len() == 2 {
        let port = parts[1];
        port.parse::<u16>()
            .map_err(|_| format!("Invalid port '{}' in proxy '{}'", port, trimmed))?;
        return Ok(format!("http://{}", trimmed));
    }

    Err(format!(
        "Unrecognized proxy format '{}'. Use http://host:port, socks5://host:port, or ip:port:user:pass",
        trimmed
    ))
}

/// A pool of proxy clients with rotation support.
pub struct ProxyPool {
    /// Direct (no-proxy) client
    direct_client: Client,
    /// Cached proxy clients keyed by proxy URL
    clients: RwLock<HashMap<String, Client>>,
    /// List of proxy URLs in the pool
    proxy_urls: RwLock<Vec<String>>,
    /// Round-robin counter
    rr_counter: AtomicUsize,
    /// Current rotation strategy
    strategy: RwLock<ProxyRotationStrategy>,
    /// Current proxy mode
    mode: RwLock<UpstreamProxyMode>,
    /// Single custom proxy URL (for Custom mode)
    custom_url: RwLock<String>,
}

impl ProxyPool {
    /// Create a new proxy pool with the given direct client and configuration.
    pub fn new(direct_client: Client, config: &UpstreamProxyConfig) -> Self {
        // Parse and validate all proxy URLs at init time
        let parsed_urls: Vec<String> = config
            .proxy_urls
            .iter()
            .filter_map(|raw| match parse_proxy_url(raw) {
                Ok(url) => Some(url),
                Err(e) => {
                    tracing::error!("Skipping invalid proxy: {}", e);
                    None
                },
            })
            .collect();

        if config.mode == UpstreamProxyMode::Pool
            && parsed_urls.is_empty()
            && !config.proxy_urls.is_empty()
        {
            tracing::error!(
                "⚠️ ALL {} proxy URLs failed validation! Requests WILL FAIL.",
                config.proxy_urls.len()
            );
        }

        tracing::info!(
            mode = ?config.mode,
            pool_size = parsed_urls.len(),
            strategy = ?config.rotation_strategy,
            "ProxyPool initialized"
        );

        Self {
            direct_client,
            clients: RwLock::new(HashMap::new()),
            proxy_urls: RwLock::new(parsed_urls),
            rr_counter: AtomicUsize::new(0),
            strategy: RwLock::new(config.rotation_strategy),
            mode: RwLock::new(config.mode),
            custom_url: RwLock::new(config.url.clone()),
        }
    }

    /// Update the pool configuration (called on hot-reload).
    pub async fn update_config(&self, config: &UpstreamProxyConfig) {
        let mut mode = self.mode.write().await;
        let mut strategy = self.strategy.write().await;
        let mut urls = self.proxy_urls.write().await;
        let mut custom = self.custom_url.write().await;

        let mode_changed = *mode != config.mode;

        // Parse new URLs
        let new_parsed: Vec<String> = config
            .proxy_urls
            .iter()
            .filter_map(|raw| match parse_proxy_url(raw) {
                Ok(url) => Some(url),
                Err(e) => {
                    tracing::error!("Skipping invalid proxy on reload: {}", e);
                    None
                },
            })
            .collect();

        let urls_changed = *urls != new_parsed;

        *mode = config.mode;
        *strategy = config.rotation_strategy;
        *custom = config.url.clone();

        if urls_changed {
            let new_set: std::collections::HashSet<&str> =
                new_parsed.iter().map(|s| s.as_str()).collect();
            let mut clients = self.clients.write().await;
            clients.retain(|url, _| new_set.contains(url.as_str()));
            *urls = new_parsed;
        }

        if mode_changed || urls_changed {
            tracing::info!(
                mode = ?config.mode,
                pool_size = urls.len(),
                strategy = ?config.rotation_strategy,
                "ProxyPool config updated"
            );
        }
    }

    /// Get or create a `reqwest::Client` for the given proxy URL.
    async fn get_or_create_client(&self, proxy_url: &str) -> Result<Client, String> {
        // Fast path: check read lock
        {
            let clients = self.clients.read().await;
            if let Some(client) = clients.get(proxy_url) {
                return Ok(client.clone());
            }
        }

        // Slow path: create under write lock
        let mut clients = self.clients.write().await;
        // Double-check after acquiring write lock
        if let Some(client) = clients.get(proxy_url) {
            return Ok(client.clone());
        }

        let proxy = reqwest::Proxy::all(proxy_url)
            .map_err(|e| format!("Invalid proxy URL '{}': {}", proxy_url, e))?;

        let proxy_url_owned = proxy_url.to_string();
        let new_client = tokio::task::spawn_blocking(move || {
            Client::builder()
                .connect_timeout(Duration::from_secs(20))
                .pool_max_idle_per_host(8)
                .pool_idle_timeout(Duration::from_secs(90))
                .tcp_keepalive(Duration::from_secs(60))
                .http2_keep_alive_interval(Duration::from_secs(25))
                .http2_keep_alive_timeout(Duration::from_secs(10))
                .http2_keep_alive_while_idle(true)
                .timeout(Duration::from_secs(600))
                .user_agent(default_user_agent())
                .proxy(proxy)
                .build()
                .map_err(|e| {
                    format!("Failed to build proxy client for '{}': {}", proxy_url_owned, e)
                })
        })
        .await
        .map_err(|e| format!("spawn_blocking panicked: {}", e))??;

        tracing::info!(proxy_url = %proxy_url, "Created new proxy client");
        clients.insert(proxy_url.to_string(), new_client.clone());
        Ok(new_client)
    }

    /// Select a proxy URL from the pool based on the rotation strategy.
    ///
    /// **STRICT**: In Pool mode, returns `Err` if pool is empty or account is missing.
    /// Never silently falls back to direct connection.
    pub async fn select_proxy_url(
        &self,
        account_email: Option<&str>,
    ) -> Result<Option<String>, String> {
        let mode = *self.mode.read().await;

        match mode {
            UpstreamProxyMode::Direct => Ok(None),
            UpstreamProxyMode::System => Ok(None),
            UpstreamProxyMode::Custom => {
                let url = self.custom_url.read().await;
                if url.is_empty() {
                    Err("Custom proxy mode enabled but URL is empty — refusing to send without proxy".to_string())
                } else {
                    Ok(Some(url.clone()))
                }
            },
            UpstreamProxyMode::Pool => {
                let urls = self.proxy_urls.read().await;
                if urls.is_empty() {
                    return Err(
                        "🚫 Proxy pool is EMPTY — refusing to send request without proxy. Add proxy URLs to config.".to_string()
                    );
                }

                let strategy = *self.strategy.read().await;
                let idx = match strategy {
                    ProxyRotationStrategy::RoundRobin => {
                        self.rr_counter.fetch_add(1, Ordering::Relaxed) % urls.len()
                    },
                    ProxyRotationStrategy::Random => {
                        use std::time::SystemTime;
                        let seed = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .subsec_nanos() as usize;
                        seed % urls.len()
                    },
                    ProxyRotationStrategy::PerAccount => match account_email {
                        Some(email) => {
                            let mut hasher = DefaultHasher::new();
                            email.hash(&mut hasher);
                            hasher.finish() as usize % urls.len()
                        },
                        None => {
                            return Err(
                                "🚫 PerAccount proxy mode requires account email but none provided"
                                    .to_string(),
                            );
                        },
                    },
                };

                let selected = urls[idx].clone();
                tracing::debug!(
                    strategy = ?strategy,
                    proxy_index = idx,
                    total = urls.len(),
                    account = account_email.unwrap_or("N/A"),
                    "🔀 Selected proxy from pool"
                );
                Ok(Some(selected))
            },
        }
    }

    /// Get a `reqwest::Client` configured for the selected proxy.
    ///
    /// **STRICT**: In Pool/Custom mode, FAILS if no proxy available.
    /// Never silently falls back to direct connection.
    pub async fn get_client(&self, account_email: Option<&str>) -> Result<Client, String> {
        match self.select_proxy_url(account_email).await? {
            Some(proxy_url) => self.get_or_create_client(&proxy_url).await,
            None => Ok(self.direct_client.clone()),
        }
    }

    /// Get the currently selected proxy URL for the given account (for logging).
    pub async fn get_proxy_url_for_account(
        &self,
        account_email: &str,
    ) -> Result<Option<String>, String> {
        self.select_proxy_url(Some(account_email)).await
    }

    /// Get pool statistics for monitoring.
    pub async fn stats(&self) -> ProxyPoolStats {
        let mode = *self.mode.read().await;
        let strategy = *self.strategy.read().await;
        let urls = self.proxy_urls.read().await;
        let clients = self.clients.read().await;

        ProxyPoolStats {
            mode,
            strategy,
            pool_size: urls.len(),
            active_clients: clients.len(),
            total_requests: self.rr_counter.load(Ordering::Relaxed),
        }
    }

    /// Get or create a client for an explicit WARP proxy URL.
    pub async fn get_or_create_warp_client(&self, proxy_url: &str) -> Result<Client, String> {
        self.get_or_create_client(proxy_url).await
    }
}

/// Statistics about the proxy pool.
#[derive(Debug, Clone)]
pub struct ProxyPoolStats {
    pub mode: UpstreamProxyMode,
    pub strategy: ProxyRotationStrategy,
    pub pool_size: usize,
    pub active_clients: usize,
    pub total_requests: usize,
}
