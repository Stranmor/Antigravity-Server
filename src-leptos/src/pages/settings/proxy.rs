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
            <p class="section-desc">"Configure how outgoing API requests are routed"</p>

            <div class="setting-row">
                <div class="setting-info">
                    <label>"Proxy mode"</label>
                    <p class="setting-desc">"Direct = no proxy, System = use ALL_PROXY/HTTP_PROXY, Custom = specify URL"</p>
                </div>
                <select
                    prop:value=move || {
                        state.config.get()
                            .map(|c| match c.proxy.upstream_proxy.mode {
                                UpstreamProxyMode::Direct => "direct",
                                UpstreamProxyMode::System => "system",
                                UpstreamProxyMode::Custom => "custom",
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
                </select>
            </div>

            <Show when=move || {
                state.config.get()
                    .map(|c| matches!(c.proxy.upstream_proxy.mode, UpstreamProxyMode::Custom))
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
        </section>
    }
}
