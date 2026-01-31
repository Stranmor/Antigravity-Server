//! Settings page

use crate::api::commands;
use crate::api_models::UpdateInfo;
use crate::app::AppState;
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;
use leptos::task::spawn_local;

const VERSION: &str = env!("GIT_VERSION");

#[component]
pub fn Settings() -> impl IntoView {
    let state = expect_context::<AppState>();

    // Saving state
    let saving = RwSignal::new(false);
    let checking_update = RwSignal::new(false);
    let update_info = RwSignal::new(Option::<UpdateInfo>::None);
    let data_path = RwSignal::new(String::new());
    let message = RwSignal::new(Option::<(String, bool)>::None);

    // Load data path on mount
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(path) = commands::get_data_dir_path().await {
                data_path.set(path);
            }
        });
    });

    let show_message = move |msg: String, is_error: bool| {
        message.set(Some((msg, is_error)));
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            message.set(None);
        });
    };

    // Clone state for action closures
    let state_for_save = state.clone();

    // Save settings
    let on_save = move || {
        saving.set(true);
        let s = state_for_save.clone();
        spawn_local(async move {
            if let Some(config) = s.config.get() {
                match commands::save_config(&config).await {
                    Ok(()) => show_message("Settings saved".to_string(), false),
                    Err(e) => show_message(format!("Save failed: {}", e), true),
                }
            }
            saving.set(false);
        });
    };

    // Check for updates
    let on_check_update = move || {
        checking_update.set(true);
        spawn_local(async move {
            match commands::check_for_updates().await {
                Ok(info) => {
                    if info.available {
                        show_message(format!("Update available: {}", info.latest_version), false);
                    } else {
                        show_message("You're up to date!".to_string(), false);
                    }
                    update_info.set(Some(info));
                }
                Err(e) => show_message(format!("Check failed: {}", e), true),
            }
            checking_update.set(false);
        });
    };

    // Open data folder
    let on_open_data = move || {
        spawn_local(async move {
            if let Err(e) = commands::open_data_folder().await {
                show_message(format!("Failed: {}", e), true);
            }
        });
    };

    // Clear logs
    let on_clear_logs = move || {
        spawn_local(async move {
            match commands::clear_log_cache().await {
                Ok(()) => show_message("Logs cleared".to_string(), false),
                Err(e) => show_message(format!("Failed: {}", e), true),
            }
        });
    };

    view! {
        <div class="page settings">
            <header class="page-header">
                <h1>"Settings"</h1>
                <Button
                    text="💾 Save".to_string()
                    variant=ButtonVariant::Primary
                    loading=saving.get()
                    on_click=on_save
                />
            </header>

            // Message banner
            <Show when=move || message.get().is_some()>
                {move || {
                    let (msg, is_error) = message.get().unwrap();
                    view! {
                        <div class=format!("alert {}", if is_error { "alert--error" } else { "alert--success" })>
                            <span>{msg}</span>
                        </div>
                    }
                }}
            </Show>

            // General
            <section class="settings-section">
                <h2>"General"</h2>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Language"</label>
                        <p class="setting-desc">"Interface language"</p>
                    </div>
                    <select
                        prop:value=move || state.config.get().map(|c| c.language.clone()).unwrap_or_default()
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.language = value;
                                }
                            });
                        }
                    >
                        <option value="en">"English"</option>
                        <option value="zh">"中文"</option>
                        <option value="ru">"Русский"</option>
                    </select>
                </div>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Theme"</label>
                        <p class="setting-desc">"Application color scheme"</p>
                    </div>
                    <select
                        prop:value=move || state.config.get().map(|c| c.theme.clone()).unwrap_or_default()
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.theme = value;
                                }
                            });
                        }
                    >
                        <option value="dark">"Dark"</option>
                        <option value="light">"Light"</option>
                        <option value="system">"System"</option>
                    </select>
                </div>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Auto-launch"</label>
                        <p class="setting-desc">"Start with system"</p>
                    </div>
                    <input
                        type="checkbox"
                        class="toggle"
                        checked=move || state.config.get().map(|c| c.auto_launch).unwrap_or(false)
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.auto_launch = checked;
                                }
                            });
                        }
                    />
                </div>
            </section>

            // Quota Refresh
            <section class="settings-section">
                <h2>"Quota Refresh"</h2>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Auto-refresh quotas"</label>
                        <p class="setting-desc">"Automatically update account quotas"</p>
                    </div>
                    <input
                        type="checkbox"
                        class="toggle"
                        checked=move || state.config.get().map(|c| c.auto_refresh).unwrap_or(true)
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.auto_refresh = checked;
                                }
                            });
                        }
                    />
                </div>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Refresh interval"</label>
                        <p class="setting-desc">"Minutes between quota updates"</p>
                    </div>
                    <input
                        type="number"
                        min="1"
                        max="1440"
                        prop:value=move || state.config.get().map(|c| c.refresh_interval.to_string()).unwrap_or_default()
                        on:change=move |ev| {
                            if let Ok(value) = event_target_value(&ev).parse::<i32>() {
                                state.config.update(|c| {
                                    if let Some(config) = c.as_mut() {
                                        config.refresh_interval = value;
                                    }
                                });
                            }
                        }
                    />
                </div>
            </section>

            // Quota Protection
            <section class="settings-section">
                <h2>"Quota Protection"</h2>
                <p class="section-desc">"Protect accounts from exhaustion by monitoring quota thresholds"</p>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Enable quota protection"</label>
                        <p class="setting-desc">"Automatically disable accounts when quota falls below threshold"</p>
                    </div>
                    <input
                        type="checkbox"
                        class="toggle"
                        checked=move || state.config.get().map(|c| c.quota_protection.enabled).unwrap_or(false)
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.quota_protection.enabled = checked;
                                }
                            });
                        }
                    />
                </div>

                <Show when=move || state.config.get().map(|c| c.quota_protection.enabled).unwrap_or(false)>
                    <div class="setting-row">
                        <div class="setting-info">
                            <label>"Threshold percentage"</label>
                            <p class="setting-desc">"Accounts below this percentage will be protected (1-99)"</p>
                        </div>
                        <input
                            type="number"
                            min="1"
                            max="99"
                            prop:value=move || state.config.get().map(|c| c.quota_protection.threshold_percentage.to_string()).unwrap_or("20".to_string())
                            on:change=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<u8>() {
                                    state.config.update(|c| {
                                        if let Some(config) = c.as_mut() {
                                            config.quota_protection.threshold_percentage = value.clamp(1, 99);
                                        }
                                    });
                                }
                            }
                        />
                    </div>

                    <div class="setting-row">
                        <div class="setting-info">
                            <label>"Auto-restore"</label>
                            <p class="setting-desc">"Automatically re-enable accounts when quota resets"</p>
                        </div>
                        <input
                            type="checkbox"
                            class="toggle"
                            checked=move || state.config.get().map(|c| c.quota_protection.auto_restore).unwrap_or(true)
                            on:change=move |ev| {
                                let checked = event_target_checked(&ev);
                                state.config.update(|c| {
                                    if let Some(config) = c.as_mut() {
                                        config.quota_protection.auto_restore = checked;
                                    }
                                });
                            }
                        />
                    </div>
                </Show>
            </section>

            // Smart Warmup
            <section class="settings-section">
                <h2>"Smart Warmup"</h2>
                <p class="section-desc">"Pre-warm accounts to maintain active sessions and optimize quotas"</p>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Enable smart warmup"</label>
                        <p class="setting-desc">"Periodically send warmup requests to keep accounts active"</p>
                    </div>
                    <input
                        type="checkbox"
                        class="toggle"
                        checked=move || state.config.get().map(|c| c.smart_warmup.enabled).unwrap_or(false)
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.smart_warmup.enabled = checked;
                                }
                            });
                        }
                    />
                </div>

                <Show when=move || state.config.get().map(|c| c.smart_warmup.enabled).unwrap_or(false)>
                    <div class="setting-row">
                        <div class="setting-info">
                            <label>"Warmup interval"</label>
                            <p class="setting-desc">"Minutes between warmup cycles (5-1440)"</p>
                        </div>
                        <input
                            type="number"
                            min="5"
                            max="1440"
                            prop:value=move || state.config.get().map(|c| c.smart_warmup.interval_minutes.to_string()).unwrap_or("60".to_string())
                            on:change=move |ev| {
                                if let Ok(value) = event_target_value(&ev).parse::<u32>() {
                                    state.config.update(|c| {
                                        if let Some(config) = c.as_mut() {
                                            config.smart_warmup.interval_minutes = value.clamp(5, 1440);
                                        }
                                    });
                                }
                            }
                        />
                    </div>

                    <div class="setting-row">
                        <div class="setting-info">
                            <label>"Only low quota accounts"</label>
                            <p class="setting-desc">"Only warmup accounts below quota threshold"</p>
                        </div>
                        <input
                            type="checkbox"
                            class="toggle"
                            checked=move || state.config.get().map(|c| c.smart_warmup.only_low_quota).unwrap_or(false)
                            on:change=move |ev| {
                                let checked = event_target_checked(&ev);
                                state.config.update(|c| {
                                    if let Some(config) = c.as_mut() {
                                        config.smart_warmup.only_low_quota = checked;
                                    }
                                });
                            }
                        />
                    </div>
                </Show>
            </section>

            // Upstream Proxy
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
                                    crate::api_models::UpstreamProxyMode::Direct => "direct",
                                    crate::api_models::UpstreamProxyMode::System => "system",
                                    crate::api_models::UpstreamProxyMode::Custom => "custom",
                                })
                                .unwrap_or("direct")
                                .to_string()
                        }
                        on:change=move |ev| {
                            let value = event_target_value(&ev);
                            state.config.update(|c| {
                                if let Some(config) = c.as_mut() {
                                    config.proxy.upstream_proxy.mode = match value.as_str() {
                                        "system" => crate::api_models::UpstreamProxyMode::System,
                                        "custom" => crate::api_models::UpstreamProxyMode::Custom,
                                        _ => crate::api_models::UpstreamProxyMode::Direct,
                                    };
                                    // Sync legacy enabled field
                                    config.proxy.upstream_proxy.enabled =
                                        !matches!(config.proxy.upstream_proxy.mode, crate::api_models::UpstreamProxyMode::Direct);
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
                        .map(|c| matches!(c.proxy.upstream_proxy.mode, crate::api_models::UpstreamProxyMode::Custom))
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

            // Paths
            <section class="settings-section">
                <h2>"Data & Storage"</h2>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Data directory"</label>
                        <p class="setting-desc">{move || data_path.get()}</p>
                    </div>
                    <Button
                        text="📁 Open".to_string()
                        variant=ButtonVariant::Secondary
                        on_click=on_open_data
                    />
                </div>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Export directory"</label>
                        <p class="setting-desc">"Default location for exported data"</p>
                    </div>
                    <div class="path-input">
                        <input
                            type="text"
                            readonly=true
                            prop:value=move || state.config.get()
                                .and_then(|c| c.default_export_path)
                                .unwrap_or_else(|| "Not set".to_string())
                        />
                        <button class="btn btn--icon">"📁"</button>
                    </div>
                </div>
            </section>

            // About
            <section class="settings-section settings-section--about">
                <h2>"About"</h2>

                <div class="about-info">
                    <div class="app-icon">"🚀"</div>
                    <div class="app-details">
                        <h3>"Antigravity Manager"</h3>
                        <p class="version-text">{format!("Version {}", VERSION)}</p>
                        <p class="links">
                            <a href="https://github.com/nicepkg/gpt-runner" target="_blank">"GitHub"</a>
                        </p>
                    </div>
                </div>

                <div class="update-section">
                    <Show when=move || update_info.get().is_some_and(|u| u.available)>
                        {move || {
                            let info = update_info.get().unwrap();
                            view! {
                                <div class="update-available">
                                    <span class="update-badge">"NEW"</span>
                                    <span>"Version "{info.latest_version}" is available"</span>
                                    {info.release_url.map(|url| view! {
                                        <a href=url target="_blank" class="btn btn--primary btn--sm">"Download"</a>
                                    })}
                                </div>
                            }
                        }}
                    </Show>
                    <Button
                        text="🔍 Check for Updates".to_string()
                        variant=ButtonVariant::Secondary
                        loading=checking_update.get()
                        on_click=on_check_update
                    />
                </div>
            </section>

            // Danger zone
            <section class="settings-section settings-section--danger">
                <h2>"Maintenance"</h2>

                <div class="setting-row">
                    <div class="setting-info">
                        <label>"Clear logs"</label>
                        <p class="setting-desc">"Remove all request logs"</p>
                    </div>
                    <Button
                        text="Clear Logs".to_string()
                        variant=ButtonVariant::Danger
                        on_click=on_clear_logs
                    />
                </div>
            </section>
        </div>
    }
}

fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.checked())
        .unwrap_or(false)
}
