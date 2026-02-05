//! Quota protection settings

use crate::app::AppState;
use leptos::prelude::*;

fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.checked())
        .unwrap_or(false)
}

/// Quota protection settings section.
#[component]
pub(crate) fn QuotaProtectionSettings() -> impl IntoView {
    let state = expect_context::<AppState>();

    view! {
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
    }
}
