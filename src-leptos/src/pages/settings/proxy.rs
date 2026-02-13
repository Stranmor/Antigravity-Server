//! Upstream proxy settings

use crate::api_models::UpstreamProxyMode;
use crate::app::AppState;
use leptos::prelude::*;

/// Upstream proxy settings section.
#[component]
pub(crate) fn UpstreamProxySettings() -> impl IntoView {
    let state = expect_context::<AppState>();

    view! {
        <section class="settings-section">
            <h2>"Upstream Proxy"</h2>
            <p class="section-desc">"Configure how outgoing API requests are routed for ban protection"</p>

            <div class="setting-row">
                <div class="setting-info">
                    <label>"Proxy mode"</label>
                    <p class="setting-desc">"Direct = no proxy, System = use ALL_PROXY/HTTP_PROXY, Custom = single proxy, Pool = rotate through multiple proxies"</p>
                </div>
                <select
                    prop:value=move || {
                        state.config.get()
                            .map(|c| match c.proxy.upstream_proxy.mode {
                                UpstreamProxyMode::Direct => "direct",
                                UpstreamProxyMode::System => "system",
                                UpstreamProxyMode::Custom => "custom",
                                UpstreamProxyMode::Pool => "pool",
                            })
                            .unwrap_or("direct")
                            .to_string()
                    }
                    on:change=move |ev| {
                        let value = event_target_value(&ev);
                        state.config.update(|c| {
                            if let Some(config) = c.as_mut() {
                                config.proxy.upstream_proxy.mode = match value.as_str() {
                                    "system" => UpstreamProxyMode::System,
                                    "custom" => UpstreamProxyMode::Custom,
                                    "pool" => UpstreamProxyMode::Pool,
                                    _ => UpstreamProxyMode::Direct,
                                };
                                config.proxy.upstream_proxy.enabled =
                                    !matches!(config.proxy.upstream_proxy.mode, UpstreamProxyMode::Direct);
                            }
                        });
                    }
                >
                    <option value="direct">"Direct (no proxy)"</option>
                    <option value="system">"System (ALL_PROXY)"</option>
                    <option value="custom">"Custom URL"</option>
                    <option value="pool">"Pool (rotate proxies)"</option>
                </select>
            </div>

            <Show when=move || {
                state.config.get()
                    .map(|c| matches!(c.proxy.upstream_proxy.mode, UpstreamProxyMode::Custom | UpstreamProxyMode::Pool))
                    .unwrap_or(false)
            }>
                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Proxy URL"</label>
                        <p class="setting-desc">"e.g. socks5://127.0.0.1:1080 or http://vps:8045"</p>
                    </div>
                    <input
                        type="text"
                        placeholder="socks5://127.0.0.1:1080"
                        prop:value=move || {
                            state.config.get()
                                .map(|c| c.proxy.upstream_proxy.url.clone())
                                .unwrap_or_default()
                        }
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.proxy.upstream_proxy.url = value;
                                }
                            });
                        }
                    />
                </div>
            </Show>

            <Show when=move || {
                state.config.get()
                    .map(|c| matches!(c.proxy.upstream_proxy.mode, UpstreamProxyMode::Pool))
                    .unwrap_or(false)
            }>
                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Proxy URLs (one per line)"</label>
                        <p class="setting-desc">"List of proxy servers for rotation. Supports socks5://, http://, https://"</p>
                    </div>
                    <textarea
                        placeholder="socks5://proxy1:1080\nhttp://proxy2:8080\nsocks5://proxy3:1080"
                        rows="5"
                        prop:value=move || {
                            state.config.get()
                                .map(|c| c.proxy.upstream_proxy.proxy_urls.join("\n"))
                                .unwrap_or_default()
                        }
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.proxy.upstream_proxy.proxy_urls = value
                                        .lines()
                                        .map(|l| l.trim().to_string())
                                        .filter(|l| !l.is_empty())
                                        .collect();
                                }
                            });
                        }
                    />
                </div>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Rotation strategy"</label>
                        <p class="setting-desc">"RoundRobin = even distribution, Random = random pick, PerAccount = sticky proxy per account"</p>
                    </div>
                    <select
                        prop:value=move || {
                            state.config.get()
                                .map(|c| {
                                    use antigravity_types::models::ProxyRotationStrategy;
                                    match c.proxy.upstream_proxy.rotation_strategy {
                                        ProxyRotationStrategy::RoundRobin => "round_robin",
                                        ProxyRotationStrategy::Random => "random",
                                        ProxyRotationStrategy::PerAccount => "per_account",
                                    }
                                })
                                .unwrap_or("round_robin")
                                .to_string()
                        }
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    use antigravity_types::models::ProxyRotationStrategy;
                                    config.proxy.upstream_proxy.rotation_strategy = match value.as_str() {
                                        "random" => ProxyRotationStrategy::Random,
                                        "per_account" => ProxyRotationStrategy::PerAccount,
                                        _ => ProxyRotationStrategy::RoundRobin,
                                    };
                                }
                            });
                        }
                    >
                        <option value="round_robin">"Round Robin"</option>
                        <option value="random">"Random"</option>
                        <option value="per_account">"Per Account (sticky)"</option>
                    </select>
                </div>
            </Show>
        </section>
    }
}
