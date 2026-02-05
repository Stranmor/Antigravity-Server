//! Settings page

pub(crate) mod about;
pub(crate) mod general;
pub(crate) mod proxy;
pub(crate) mod quota_protection;
pub(crate) mod quota_refresh;
pub(crate) mod warmup;

use about::{AboutSection, DataStorageSettings, MaintenanceSection};
use general::GeneralSettings;
use proxy::UpstreamProxySettings;
use quota_protection::QuotaProtectionSettings;
use quota_refresh::QuotaRefreshSettings;
use warmup::SmartWarmupSettings;

use crate::api::commands;
use crate::api_models::UpdateInfo;
use crate::app::AppState;
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Settings page for application configuration.
#[component]
pub(crate) fn Settings() -> impl IntoView {
    let state = expect_context::<AppState>();

    let saving = RwSignal::new(false);
    let checking_update = RwSignal::new(false);
    let update_info = RwSignal::new(Option::<UpdateInfo>::None);
    let data_path = RwSignal::new(String::new());
    let message = RwSignal::new(Option::<(String, bool)>::None);

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

    let state_for_save = state.clone();

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
                },
                Err(e) => show_message(format!("Check failed: {}", e), true),
            }
            checking_update.set(false);
        });
    };

    let on_open_data = move || {
        spawn_local(async move {
            if let Err(e) = commands::open_data_folder().await {
                show_message(format!("Failed: {}", e), true);
            }
        });
    };

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

            <Show when=move || message.get().is_some()>
                {move || {
                    let Some((msg, is_error)) = message.get() else {
                        return view! { <div></div> }.into_any();
                    };
                    view! {
                        <div class=format!("alert {}", if is_error { "alert--error" } else { "alert--success" })>
                            <span>{msg}</span>
                        </div>
                    }.into_any()
                }}
            </Show>

            <GeneralSettings />
            <QuotaRefreshSettings />
            <QuotaProtectionSettings />
            <SmartWarmupSettings />
            <UpstreamProxySettings />
            <DataStorageSettings data_path=data_path on_open_data=on_open_data />
            <AboutSection checking_update=checking_update update_info=update_info on_check_update=on_check_update />
            <MaintenanceSection on_clear_logs=on_clear_logs />
        </div>
    }
}
