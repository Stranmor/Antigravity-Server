//! Quota settings (refresh interval, protection)

use crate::app::AppState;
use leptos::prelude::*;

fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.checked())
        .unwrap_or(false)
}

#[component]
pub fn QuotaRefreshSettings() -> impl IntoView {
    let state = expect_context::<AppState>();

    view! {
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
    }
}
