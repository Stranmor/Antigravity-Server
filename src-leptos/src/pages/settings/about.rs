//! Data, storage, about and maintenance sections

use crate::api_models::UpdateInfo;
use crate::app::AppState;
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;

const VERSION: &str = env!("GIT_VERSION");

#[component]
pub fn DataStorageSettings(
    data_path: RwSignal<String>,
    on_open_data: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let state = expect_context::<AppState>();

    view! {
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
    }
}

#[component]
pub fn AboutSection(
    checking_update: RwSignal<bool>,
    update_info: RwSignal<Option<UpdateInfo>>,
    on_check_update: impl Fn() + 'static + Clone,
) -> impl IntoView {
    view! {
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
    }
}

#[component]
pub fn MaintenanceSection(on_clear_logs: impl Fn() + 'static + Clone) -> impl IntoView {
    view! {
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
    }
}
