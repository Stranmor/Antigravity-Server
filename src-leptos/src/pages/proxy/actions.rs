//! Action handlers for proxy page

use super::state::ProxyState;
use crate::api::commands;
use crate::app::AppState;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;

pub(crate) fn show_message(message: RwSignal<Option<(String, bool)>>, msg: String, is_error: bool) {
    message.set(Some((msg, is_error)));
    spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(3000).await;
        message.set(None);
    });
}

pub(crate) fn on_toggle(app_state: AppState, loading: RwSignal<bool>) {
    loading.set(true);
    let status = app_state.proxy_status;
    spawn_local(async move {
        let current = status.get();
        let result = if current.running {
            commands::stop_proxy_service().await
        } else {
            commands::start_proxy_service().await.map(|_| ())
        };

        if result.is_ok() {
            if let Ok(new_status) = commands::get_proxy_status().await {
                status.set(new_status);
            }
        }
        loading.set(false);
    });
}

pub(crate) fn on_save_config(ps: ProxyState) {
    spawn_local(async move {
        if let Ok(mut config) = commands::load_config().await {
            config.proxy.port = ps.port.get();
            config.proxy.request_timeout = ps.timeout.get() as u64;
            config.proxy.auto_start = ps.auto_start.get();
            config.proxy.allow_lan_access = ps.allow_lan.get();
            config.proxy.auth_mode = ps.auth_mode.get();
            config.proxy.enable_logging = ps.enable_logging.get();
            config.proxy.custom_mapping = ps.custom_mappings.get_untracked();
            config.proxy.zai.enabled = ps.zai_enabled.get();
            config.proxy.zai.base_url = ps.zai_base_url.get_untracked();
            config.proxy.zai.api_key = ps.zai_api_key.get_untracked();
            config.proxy.zai.dispatch_mode = ps.zai_dispatch_mode.get_untracked();
            config.proxy.zai.model_mapping = ps.zai_model_mapping.get_untracked();

            if commands::save_config(&config).await.is_ok() {
                show_message(ps.message, "Configuration saved".to_string(), false);
            }
        }
    });
}

pub(crate) fn on_generate_key(
    api_key: RwSignal<String>,
    message: RwSignal<Option<(String, bool)>>,
) {
    spawn_local(async move {
        if let Ok(key) = commands::generate_api_key().await {
            api_key.set(key);
            show_message(message, "API key regenerated".to_string(), false);
        }
    });
}

pub(crate) fn on_copy(text: String, label: String, copied: RwSignal<Option<String>>) {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let _ = clipboard.write_text(&text);
    }
    copied.set(Some(label));
    spawn_local(async move {
        gloo_timers::future::TimeoutFuture::new(2000).await;
        copied.set(None);
    });
}

pub(crate) fn on_add_mapping(
    new_from: RwSignal<String>,
    new_to: RwSignal<String>,
    mappings: RwSignal<HashMap<String, String>>,
) {
    let from = new_from.get();
    let to = new_to.get();
    if !from.is_empty() && !to.is_empty() {
        mappings.update(|m| {
            m.insert(from.clone(), to.clone());
        });
        new_from.set(String::new());
        new_to.set(String::new());
    }
}

pub(crate) fn on_apply_presets(
    mappings: RwSignal<HashMap<String, String>>,
    message: RwSignal<Option<(String, bool)>>,
) {
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

    mappings.update(|m| m.extend(presets));
    show_message(message, "Presets applied".to_string(), false);
}

pub(crate) fn on_clear_bindings(message: RwSignal<Option<(String, bool)>>) {
    spawn_local(async move {
        if commands::clear_proxy_session_bindings().await.is_ok() {
            show_message(message, "Session bindings cleared".to_string(), false);
        }
    });
}

pub(crate) fn on_test_mapping(ps: ProxyState) {
    let model = ps.test_model_input.get();
    if model.is_empty() {
        return;
    }

    ps.test_loading.set(true);
    ps.test_result.set(None);

    spawn_local(async move {
        match commands::detect_model(&model).await {
            Ok(res) => {
                ps.test_result.set(Some(res));
            },
            Err(e) => {
                show_message(ps.message, format!("Test failed: {}", e), true);
            },
        }
        ps.test_loading.set(false);
    });
}

pub(crate) fn load_config_on_mount(ps: ProxyState) {
    spawn_local(async move {
        if let Ok(config) = commands::load_config().await {
            ps.port.set(config.proxy.port);
            ps.timeout.set(config.proxy.request_timeout as u32);
            ps.auto_start.set(config.proxy.auto_start);
            ps.allow_lan.set(config.proxy.allow_lan_access);
            ps.auth_mode.set(config.proxy.auth_mode);
            ps.api_key.set(config.proxy.api_key.clone());
            ps.enable_logging.set(config.proxy.enable_logging);
            ps.custom_mappings.set(config.proxy.custom_mapping.clone());
            ps.zai_enabled.set(config.proxy.zai.enabled);
            ps.zai_base_url.set(config.proxy.zai.base_url.clone());
            ps.zai_api_key.set(config.proxy.zai.api_key.clone());
            ps.zai_dispatch_mode.set(config.proxy.zai.dispatch_mode);
            ps.zai_model_mapping.set(config.proxy.zai.model_mapping.clone());
        }
    });
}
