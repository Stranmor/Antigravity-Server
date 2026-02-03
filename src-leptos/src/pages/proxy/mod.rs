mod actions;
mod quick_start;
mod routing;
mod scheduling;
mod state;
mod test_mapping;
mod zai;

use actions::{load_config_on_mount, on_copy, on_generate_key, on_save_config, on_toggle};
use quick_start::QuickStart;
use routing::ModelRouting;
use scheduling::Scheduling;
use state::ProxyState;
use test_mapping::TestMapping;
use zai::ZaiProvider;

use crate::api_models::ProxyAuthMode;
use crate::app::AppState;
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;

#[component]
pub fn ApiProxy() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let ps = ProxyState::new();

    Effect::new({
        let ps = ps.clone();
        move |_| load_config_on_mount(ps.clone())
    });

    let status = app_state.proxy_status;
    let loading = ps.loading;
    let message = ps.message;
    let port = ps.port;
    let timeout = ps.timeout;
    let auto_start = ps.auto_start;
    let allow_lan = ps.allow_lan;
    let auth_mode = ps.auth_mode;
    let api_key = ps.api_key;
    let enable_logging = ps.enable_logging;
    let copied = ps.copied;

    let ps_save = ps.clone();
    let ps_test = ps.clone();

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
                    <button
                        class=move || format!("btn {}", if status.get().running { "btn--danger" } else { "btn--primary" })
                        disabled=move || loading.get()
                        on:click=move |_| on_toggle(app_state.clone(), loading)
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

            <div class="config-card">
                <div class="config-header">
                    <h2>"🔧 Configuration"</h2>
                    <Button
                        text="💾 Save".to_string()
                        variant=ButtonVariant::Primary
                        on_click=move || on_save_config(ps_save.clone())
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
                                move |_| on_copy(key.clone(), "api_key".to_string(), copied)
                            }
                        >
                            {move || if copied.get() == Some("api_key".to_string()) { "✓" } else { "📋" }}
                        </button>
                        <button
                            class="btn btn--icon"
                            title="Regenerate"
                            on:click=move |_| on_generate_key(api_key, message)
                        >"🔄"</button>
                    </div>
                </div>
            </div>

            <ModelRouting
                routing_expanded=ps.routing_expanded
                custom_mappings=ps.custom_mappings
                new_mapping_from=ps.new_mapping_from
                new_mapping_to=ps.new_mapping_to
                message=ps.message
            />

            <TestMapping ps=ps_test />

            <Scheduling
                scheduling_expanded=ps.scheduling_expanded
                scheduling_mode=ps.scheduling_mode
                sticky_session_ttl=ps.sticky_session_ttl
                message=ps.message
            />

            <ZaiProvider
                zai_expanded=ps.zai_expanded
                zai_enabled=ps.zai_enabled
                zai_base_url=ps.zai_base_url
                zai_api_key=ps.zai_api_key
                zai_dispatch_mode=ps.zai_dispatch_mode
                zai_model_mapping=ps.zai_model_mapping
                new_mapping_from=ps.new_mapping_from
                new_mapping_to=ps.new_mapping_to
            />

            <QuickStart
                port=ps.port
                api_key=ps.api_key
                selected_protocol=ps.selected_protocol
                selected_model=ps.selected_model
                copied=ps.copied
            />
        </div>
    }
}
