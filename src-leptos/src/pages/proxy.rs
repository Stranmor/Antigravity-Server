//! API Proxy page with full parity

use crate::api::commands;
use crate::api_models::{Protocol, ProxyAuthMode, ZaiDispatchMode};
use crate::app::AppState;
use crate::components::{Button, ButtonVariant, CollapsibleCard, Select};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;

#[component]
pub fn ApiProxy() -> impl IntoView {
    let state = expect_context::<AppState>();

    // Local state
    let loading = RwSignal::new(false);
    let copied = RwSignal::new(Option::<String>::None);
    let selected_protocol = RwSignal::new(Protocol::OpenAI);
    let selected_model = RwSignal::new("gemini-3-flash".to_string());

    // Config state
    let port = RwSignal::new(8045u16);
    let timeout = RwSignal::new(120u32);
    let auto_start = RwSignal::new(false);
    let allow_lan = RwSignal::new(false);
    let auth_mode = RwSignal::new(ProxyAuthMode::default());
    let api_key = RwSignal::new(String::new());
    let enable_logging = RwSignal::new(true);

    // Model mapping state
    let custom_mappings = RwSignal::new(HashMap::<String, String>::new());
    let new_mapping_from = RwSignal::new(String::new());
    let new_mapping_to = RwSignal::new(String::new());

    // Scheduling state
    let scheduling_mode = RwSignal::new("balance".to_string());
    let sticky_session_ttl = RwSignal::new(3600u32);

    // Z.ai State
    let zai_expanded = RwSignal::new(false);
    let zai_enabled = RwSignal::new(false);
    let zai_base_url = RwSignal::new(String::new());
    let zai_api_key = RwSignal::new(String::new());
    let zai_dispatch_mode = RwSignal::new(ZaiDispatchMode::default());
    let zai_model_mapping = RwSignal::new(HashMap::<String, String>::new());
    let routing_expanded = RwSignal::new(true);
    let scheduling_expanded = RwSignal::new(false);

    // Test Mapping State
    let test_mapping_expanded = RwSignal::new(false);
    let test_model_input = RwSignal::new(String::new());
    let test_result = RwSignal::new(Option::<crate::api::commands::ModelDetectResponse>::None);
    let test_loading = RwSignal::new(false);

    // Message
    let message = RwSignal::new(Option::<(String, bool)>::None);

    let show_message = move |msg: String, is_error: bool| {
        message.set(Some((msg, is_error)));
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            message.set(None);
        });
    };

    // Load config on mount
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(config) = commands::load_config().await {
                port.set(config.proxy.port);
                timeout.set(config.proxy.request_timeout as u32);
                auto_start.set(config.proxy.auto_start);
                allow_lan.set(config.proxy.allow_lan_access);
                auth_mode.set(config.proxy.auth_mode.clone());
                api_key.set(config.proxy.api_key.clone());
                enable_logging.set(config.proxy.enable_logging);
                custom_mappings.set(config.proxy.custom_mapping.clone());
                zai_enabled.set(config.proxy.zai.enabled);
                zai_base_url.set(config.proxy.zai.base_url.clone());
                zai_api_key.set(config.proxy.zai.api_key.clone());
                zai_dispatch_mode.set(config.proxy.zai.dispatch_mode);
                zai_model_mapping.set(config.proxy.zai.model_mapping.clone());
            }
        });
    });

    // Status shortcut
    let status = state.proxy_status;

    // Clone state for action closures
    let state_toggle = state.clone();

    // Toggle proxy
    let on_toggle = move || {
        loading.set(true);
        let s = state_toggle.clone();
        spawn_local(async move {
            let current = status.get();
            let result = if current.running {
                commands::stop_proxy_service().await
            } else {
                commands::start_proxy_service().await.map(|_| ())
            };

            if result.is_ok() {
                if let Ok(new_status) = commands::get_proxy_status().await {
                    s.proxy_status.set(new_status);
                }
            }
            loading.set(false);
        });
    };

    let on_save_config = move || {
        spawn_local(async move {
            if let Ok(mut config) = commands::load_config().await {
                config.proxy.port = port.get();
                config.proxy.request_timeout = timeout.get() as u64;
                config.proxy.auto_start = auto_start.get();
                config.proxy.allow_lan_access = allow_lan.get();
                config.proxy.auth_mode = auth_mode.get();
                config.proxy.enable_logging = enable_logging.get();
                config.proxy.custom_mapping = custom_mappings.get_untracked();
                config.proxy.zai.enabled = zai_enabled.get();
                config.proxy.zai.base_url = zai_base_url.get_untracked();
                config.proxy.zai.api_key = zai_api_key.get_untracked();
                config.proxy.zai.dispatch_mode = zai_dispatch_mode.get_untracked();
                config.proxy.zai.model_mapping = zai_model_mapping.get_untracked();

                if commands::save_config(&config).await.is_ok() {
                    show_message("Configuration saved".to_string(), false);
                }
            }
        });
    };

    let on_generate_key = move || {
        spawn_local(async move {
            if let Ok(key) = commands::generate_api_key().await {
                api_key.set(key);
                show_message("API key regenerated".to_string(), false);
            }
        });
    };

    let on_copy = move |text: String, label: String| {
        if let Some(window) = web_sys::window() {
            let clipboard = window.navigator().clipboard();
            let _ = clipboard.write_text(&text);
        }
        copied.set(Some(label.clone()));
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(2000).await;
            copied.set(None);
        });
    };

    let on_add_mapping = move || {
        let from = new_mapping_from.get();
        let to = new_mapping_to.get();
        if !from.is_empty() && !to.is_empty() {
            custom_mappings.update(|m| {
                m.insert(from.clone(), to.clone());
            });
            new_mapping_from.set(String::new());
            new_mapping_to.set(String::new());
        }
    };

    let on_remove_mapping = move |key: String| {
        custom_mappings.update(|m| {
            m.remove(&key);
        });
    };

    let on_apply_presets = move || {
        let presets: HashMap<String, String> = [
            ("claude-opus-4-5", "claude-opus-4-5-thinking"),
            ("claude-haiku-4-5", "gemini-3-flash"),
            ("gemini-3-flash-high", "gemini-3-flash"),
            ("gemini-3-flash-preview", "gemini-3-flash"),
            ("claude-opus-4-5-20251101", "claude-opus-4-5-thinking"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        custom_mappings.update(|m| m.extend(presets));
        show_message("Presets applied".to_string(), false);
    };

    let on_reset_mappings = move || {
        custom_mappings.set(HashMap::new());
        show_message("Mappings reset".to_string(), false);
    };

    let on_clear_bindings = move || {
        spawn_local(async move {
            if commands::clear_proxy_session_bindings().await.is_ok() {
                show_message("Session bindings cleared".to_string(), false);
            }
        });
    };

    let on_test_mapping = move || {
        let model = test_model_input.get();
        if model.is_empty() {
            return;
        }

        test_loading.set(true);
        test_result.set(None);

        spawn_local(async move {
            match commands::detect_model(&model).await {
                Ok(res) => {
                    test_result.set(Some(res));
                }
                Err(e) => {
                    show_message(format!("Test failed: {}", e), true);
                }
            }
            test_loading.set(false);
        });
    };

    // Generate code example
    let get_example = move || {
        let p = port.get();
        let key = api_key.get();
        let model = selected_model.get();
        let base_url = format!("http://127.0.0.1:{}/v1", p);

        match selected_protocol.get() {
            Protocol::OpenAI => format!(
                r#"from openai import OpenAI

client = OpenAI(
    base_url="{}",
    api_key="{}"
)

response = client.chat.completions.create(
    model="{}",
    messages=[{{"role": "user", "content": "Hello"}}]
)

print(response.choices[0].message.content)"#,
                base_url, key, model
            ),
            Protocol::Anthropic => format!(
                r#"from anthropic import Anthropic

client = Anthropic(
    base_url="http://127.0.0.1:{}",
    api_key="{}"
)

response = client.messages.create(
    model="{}",
    max_tokens=1024,
    messages=[{{"role": "user", "content": "Hello"}}]
)

print(response.content[0].text)"#,
                p, key, model
            ),
            Protocol::Gemini => format!(
                r#"import google.generativeai as genai

genai.configure(
    api_key="{}",
    transport='rest',
    client_options={{'api_endpoint': 'http://127.0.0.1:{}'}}
)

model = genai.GenerativeModel('{}')
response = model.generate_content("Hello")
print(response.text)"#,
                key, p, model
            ),
        }
    };

    let models = [
        ("gemini-3-flash", "Gemini 3 Flash"),
        ("gemini-3-pro-high", "Gemini 3 Pro"),
        ("claude-sonnet-4-5", "Claude Sonnet 4.5"),
        ("gemini-2.5-flash", "Gemini 2.5 Flash"),
    ];

    view! {
        <div class="page proxy">
            <header class="page-header">
                <div class="header-left">
                    <h1>"API Proxy"</h1>
                    <p class="subtitle">"OpenAI-compatible API endpoint"</p>
                </div>
                <div class="header-status">
                    <span class=move || format!("status-indicator {}", if status.get().running { "running" } else { "stopped" })>
                        <span class="status-dot"></span>
                        {move || if status.get().running {
                            format!("Running ({} accounts)", status.get().active_accounts)
                        } else {
                            "Stopped".to_string()
                        }}
                    </span>
                    // Dynamic button - use native for dynamic variant
                    <button
                        class=move || format!("btn {}", if status.get().running { "btn--danger" } else { "btn--primary" })
                        disabled=move || loading.get()
                        on:click=move |_| on_toggle()
                    >
                        {move || if loading.get() {
                            "...".to_string()
                        } else if status.get().running {
                            "⏹ Stop".to_string()
                        } else {
                            "▶ Start".to_string()
                        }}
                    </button>
                </div>
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

            // Main config card
            <div class="config-card">
                <div class="config-header">
                    <h2>"🔧 Configuration"</h2>
                    <Button
                        text="💾 Save".to_string()
                        variant=ButtonVariant::Primary
                        on_click=on_save_config
                    />
                </div>

                <div class="config-grid">
                    <div class="form-group">
                        <label>"Port"</label>
                        <input
                            type="number"
                            min="1024" max="65535"
                            prop:value=move || port.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<u16>() {
                                    port.set(v);
                                }
                            }
                            disabled=move || status.get().running
                        />
                    </div>

                    <div class="form-group">
                        <label>"Timeout (s)"</label>
                        <input
                            type="number"
                            min="30" max="3600"
                            prop:value=move || timeout.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                    timeout.set(v);
                                }
                            }
                        />
                    </div>

                    <div class="form-group form-group--toggle">
                        <label>"Auto-start"</label>
                        <input
                            type="checkbox"
                            class="toggle"
                            prop:checked=move || auto_start.get()
                            on:change=move |ev| auto_start.set(event_target_checked(&ev))
                        />
                    </div>

                    <div class="form-group form-group--toggle">
                        <label>"Logging"</label>
                        <input
                            type="checkbox"
                            class="toggle"
                            prop:checked=move || enable_logging.get()
                            on:change=move |ev| enable_logging.set(event_target_checked(&ev))
                        />
                    </div>
                </div>

                // Access Control
                <div class="config-section">
                    <h3>"🔐 Access Control"</h3>
                    <div class="config-grid">
                        <div class="form-group form-group--toggle">
                            <label>"Allow LAN"</label>
                            <input
                                type="checkbox"
                                class="toggle"
                                prop:checked=move || allow_lan.get()
                                on:change=move |ev| allow_lan.set(event_target_checked(&ev))
                            />
                        </div>

                        <div class="form-group">
                            <label>"Auth Mode"</label>
                            <select
                                prop:value=move || auth_mode.get().to_string()
                                on:change=move |ev| auth_mode.set(ProxyAuthMode::from_string(&event_target_value(&ev)))
                            >
                                <option value="off">"Off"</option>
                                <option value="strict">"Strict"</option>
                                <option value="all_except_health">"All except /health"</option>
                                <option value="auto">"Auto"</option>
                            </select>
                        </div>
                    </div>
                </div>

                // API Key
                <div class="config-section">
                    <h3>"🔑 API Key"</h3>
                    <div class="api-key-row">
                        <input
                            type="text"
                            readonly=true
                            prop:value=move || api_key.get()
                            class="api-key-input"
                        />
                        <button
                            class="btn btn--icon"
                            title="Copy"
                            on:click={
                                let key = api_key.get_untracked();
                                move |_| on_copy(key.clone(), "api_key".to_string())
                            }
                        >
                            {move || if copied.get() == Some("api_key".to_string()) { "✓" } else { "📋" }}
                        </button>
                        <button
                            class="btn btn--icon"
                            title="Regenerate"
                            on:click=move |_| on_generate_key()
                        >"🔄"</button>
                    </div>
                </div>
            </div>

            // Model Routing
            <div class="config-card collapsible">
                <div class="config-header clickable" on:click=move |_| routing_expanded.update(|v| *v = !*v)>
                    <h2>"🔀 Model Routing"</h2>
                    <span class=move || format!("expand-icon {}", if routing_expanded.get() { "expanded" } else { "" })>"▼"</span>
                </div>

                <Show when=move || routing_expanded.get()>
                    <div class="config-content">
                        <div class="mapping-actions">
                            <Button
                                text="✨ Presets".to_string()
                                variant=ButtonVariant::Secondary
                                on_click=on_apply_presets
                            />
                            <Button
                                text="🗑 Reset".to_string()
                                variant=ButtonVariant::Danger
                                on_click=on_reset_mappings
                            />
                        </div>

                        <div class="mapping-form">
                            <input
                                type="text"
                                placeholder="From (e.g., gpt-4*)"
                                prop:value=move || new_mapping_from.get()
                                on:input=move |ev| new_mapping_from.set(event_target_value(&ev))
                            />
                            <span class="arrow">"→"</span>
                            <input
                                type="text"
                                placeholder="To (e.g., gemini-3-flash)"
                                prop:value=move || new_mapping_to.get()
                                on:input=move |ev| new_mapping_to.set(event_target_value(&ev))
                            />
                            <Button
                                text="➕".to_string()
                                variant=ButtonVariant::Primary
                                on_click=on_add_mapping
                            />
                        </div>

                        <div class="mapping-list">
                            {move || {
                                let mappings: Vec<_> = custom_mappings.get().into_iter().collect();
                                if mappings.is_empty() {
                                    view! { <p class="empty-text">"No custom mappings"</p> }.into_any()
                                } else {
                                    mappings.into_iter().map(|(from, to)| {
                                        let from_clone = from.clone();
                                        let from_display = from.clone();
                                        view! {
                                            <div class="mapping-item">
                                                <span class="mapping-from">{from_display}</span>
                                                <span class="mapping-arrow">"→"</span>
                                                <span class="mapping-to">{to}</span>
                                                <button
                                                    class="btn btn--icon btn--sm"
                                                    on:click=move |_| on_remove_mapping(from_clone.clone())
                                                >"×"</button>
                                            </div>
                                        }
                                    }).collect_view().into_any()
                                }
                            }}
                        </div>

                    </div>
                </Show>
            </div>

            // Test Mapping
            <div class="config-card collapsible">
                <div class="config-header clickable" on:click=move |_| test_mapping_expanded.update(|v| *v = !*v)>
                    <h2>"🧪 Test Mapping"</h2>
                    <span class=move || format!("expand-icon {}", if test_mapping_expanded.get() { "expanded" } else { "" })>"▼"</span>
                </div>

                <Show when=move || test_mapping_expanded.get()>
                    <div class="config-content">
                        <div class="form-group">
                            <label>"Enter Model Name"</label>
                            <div class="api-key-row">
                                <input
                                    type="text"
                                    placeholder="e.g. gpt-4-turbo"
                                    prop:value=move || test_model_input.get()
                                    on:input=move |ev| test_model_input.set(event_target_value(&ev))
                                    on:keydown=move |ev| {
                                        if ev.key() == "Enter" {
                                            on_test_mapping();
                                        }
                                    }
                                    class="api-key-input"
                                />
                                <button
                                    class="btn btn--primary"
                                    disabled=move || test_loading.get()
                                    on:click=move |_| on_test_mapping()
                                >
                                    {move || if test_loading.get() { "...".to_string() } else { "Test".to_string() }}
                                </button>
                            </div>
                        </div>

                        <Show when=move || test_result.get().is_some()>
                            {move || {
                                let res = test_result.get().unwrap();
                                view! {
                                    <div class="config-section" style="margin-top: 1rem; padding: 1rem; background: var(--bg-secondary); border-radius: 8px;">
                                        <div class="result-row" style="margin-bottom: 0.5rem; display: flex; justify-content: space-between;">
                                            <span class="label" style="opacity: 0.7;">"Mapped To:"</span>
                                            <span class="value code" style="font-family: monospace; font-weight: bold; color: var(--accent-primary);">{res.mapped_model}</span>
                                        </div>
                                        <div class="result-row" style="display: flex; justify-content: space-between;">
                                            <span class="label" style="opacity: 0.7;">"Reason:"</span>
                                            <span class="value">{res.mapping_reason}</span>
                                        </div>
                                    </div>
                                }
                            }}
                        </Show>
                    </div>
                </Show>
            </div>

            // Scheduling
            <div class="config-card collapsible">
                <div class="config-header clickable" on:click=move |_| scheduling_expanded.update(|v| *v = !*v)>
                    <h2>"⚖️ Load Balancing"</h2>
                    <span class=move || format!("expand-icon {}", if scheduling_expanded.get() { "expanded" } else { "" })>"▼"</span>
                </div>

                <Show when=move || scheduling_expanded.get()>
                    <div class="config-content">
                        <div class="config-grid">
                            <div class="form-group">
                                <label>"Mode"</label>
                                <select
                                    prop:value=move || scheduling_mode.get()
                                    on:change=move |ev| scheduling_mode.set(event_target_value(&ev))
                                >
                                    <option value="balance">"Balance"</option>
                                    <option value="priority">"Priority"</option>
                                    <option value="sticky">"Sticky"</option>
                                </select>
                            </div>

                            <div class="form-group">
                                <label>"Sticky TTL (s)"</label>
                                <input
                                    type="number"
                                    min="60" max="86400"
                                    prop:value=move || sticky_session_ttl.get().to_string()
                                    on:input=move |ev| {
                                        if let Ok(v) = event_target_value(&ev).parse::<u32>() {
                                            sticky_session_ttl.set(v);
                                        }
                                    }
                                />
                            </div>
                        </div>

                        <Button
                            text="🗑 Clear Bindings".to_string()
                            variant=ButtonVariant::Secondary
                            on_click=on_clear_bindings
                        />
                    </div>
                </Show>
            </div>

            // Z.ai Provider
            <CollapsibleCard title="Z.ai Provider".to_string() initial_expanded=zai_expanded.get_untracked()>
                <div class="config-content">
                    <div class="form-group form-group--toggle">
                        <label>"Enable Z.ai"</label>
                        <input
                            type="checkbox"
                            class="toggle"
                            prop:checked=move || zai_enabled.get()
                            on:change=move |ev| zai_enabled.set(event_target_checked(&ev))
                        />
                    </div>

                    <Show when=move || zai_enabled.get()>
                        <div class="config-grid">
                            <div class="form-group">
                                <label>"Base URL"</label>
                                <input
                                    type="text"
                                    placeholder="https://api.z.ai/api/anthropic"
                                    prop:value=move || zai_base_url.get()
                                    on:input=move |ev| zai_base_url.set(event_target_value(&ev))
                                />
                            </div>
                            <div class="form-group">
                                <label>"API Key"</label>
                                <input
                                    type="password"
                                    placeholder="sk-..."
                                    prop:value=move || zai_api_key.get()
                                    on:input=move |ev| zai_api_key.set(event_target_value(&ev))
                                />
                            </div>
                            <div class="form-group">
                                <label>"Dispatch Mode"</label>
                                <Select
                                    options=Signal::derive(move || {
                                        vec![
                                            ("off".to_string(), "Off".to_string()),
                                            ("exclusive".to_string(), "Exclusive".to_string()),
                                            ("pooled".to_string(), "Pooled".to_string()),
                                            ("fallback".to_string(), "Fallback".to_string()),
                                        ]
                                    })
                                    value=Signal::derive(move || zai_dispatch_mode.get().to_string())
                                    on_change=Callback::new(move |val: String| {
                                        let mode = match val.as_str() {
                                            "off" => ZaiDispatchMode::Off,
                                            "exclusive" => ZaiDispatchMode::Exclusive,
                                            "pooled" => ZaiDispatchMode::Pooled,
                                            "fallback" => ZaiDispatchMode::Fallback,
                                            _ => ZaiDispatchMode::Off,
                                        };
                                        zai_dispatch_mode.set(mode);
                                    })
                                />
                            </div>
                        </div>

                        // Z.ai Model Mapping (similar to custom_mappings)
                        <h3>"Z.ai Model Mapping"</h3>
                        <div class="mapping-form">
                            <input
                                type="text"
                                placeholder="From (e.g., claude-3-opus*)"
                                prop:value=move || new_mapping_from.get()
                                on:input=move |ev| new_mapping_from.set(event_target_value(&ev))
                            />
                            <span class="arrow">"→"</span>
                            <input
                                type="text"
                                placeholder="To (e.g., glm-4.7)"
                                prop:value=move || new_mapping_to.get()
                                on:input=move |ev| new_mapping_to.set(event_target_value(&ev))
                            />
                            <Button
                                text="➕".to_string()
                                variant=ButtonVariant::Primary
                                on_click={
                                    let new_mapping_from = new_mapping_from;
                                    let new_mapping_to = new_mapping_to;
                                    move || {
                                        let from = new_mapping_from.get_untracked();
                                        let to = new_mapping_to.get_untracked();
                                        if !from.is_empty() && !to.is_empty() {
                                            zai_model_mapping.update(|m| {
                                                m.insert(from.clone(), to.clone());
                                            });
                                            new_mapping_from.set(String::new());
                                            new_mapping_to.set(String::new());
                                        }
                                    }
                                }
                            />
                        </div>

                        <div class="mapping-list">
                            {move || {
                                let mappings: Vec<_> = zai_model_mapping.get().into_iter().collect();
                                if mappings.is_empty() {
                                    view! { <p class="empty-text">"No Z.ai model mappings"</p> }.into_any()
                                } else {
                                    mappings.into_iter().map(|(from, to)| {
                                        let from_clone = from.clone();
                                        let from_display = from.clone();
                                        view! {
                                            <div class="mapping-item">
                                                <span class="mapping-from">{from_display}</span>
                                                <span class="mapping-arrow">"→"</span>
                                                <span class="mapping-to">{to}</span>
                                                <button
                                                    class="btn btn--icon btn--sm"
                                                    on:click=move |_| zai_model_mapping.update(|m| { m.remove(&from_clone); })
                                                >"×"</button>
                                            </div>
                                        }
                                    }).collect_view().into_any()
                                }
                            }}
                        </div>
                    </Show>
                </div>
            </CollapsibleCard>

            // Quick Start
            <div class="config-card">
                <div class="config-header">
                    <h2>"🚀 Quick Start"</h2>
                </div>

                <div class="quick-start">
                    <div class="protocol-tabs">
                        <button
                            class=move || if matches!(selected_protocol.get(), Protocol::OpenAI) { "active" } else { "" }
                            on:click=move |_| selected_protocol.set(Protocol::OpenAI)
                        >"OpenAI"</button>
                        <button
                            class=move || if matches!(selected_protocol.get(), Protocol::Anthropic) { "active" } else { "" }
                            on:click=move |_| selected_protocol.set(Protocol::Anthropic)
                        >"Anthropic"</button>
                        <button
                            class=move || if matches!(selected_protocol.get(), Protocol::Gemini) { "active" } else { "" }
                            on:click=move |_| selected_protocol.set(Protocol::Gemini)
                        >"Gemini"</button>
                    </div>

                    <div class="model-selector">
                        <label>"Model:"</label>
                        <select
                            prop:value=move || selected_model.get()
                            on:change=move |ev| selected_model.set(event_target_value(&ev))
                        >
                            {models.iter().map(|(id, name)| {
                                view! { <option value=*id>{*name}</option> }
                            }).collect_view()}
                        </select>
                    </div>

                    <div class="code-block">
                        <div class="code-header">
                            <span>"Python"</span>
                            <button
                                class="btn btn--icon btn--sm"
                                on:click=move |_| {
                                    let code = get_example();
                                    on_copy(code, "code".to_string());
                                }
                            >
                                {move || if copied.get() == Some("code".to_string()) { "✓" } else { "Copy" }}
                            </button>
                        </div>
                        <pre class="code">{get_example}</pre>
                    </div>

                    <div class="base-url">
                        <label>"Base URL:"</label>
                        <code>{move || format!("http://127.0.0.1:{}/v1", port.get())}</code>
                    </div>
                </div>
            </div>
        </div>
    }
}
