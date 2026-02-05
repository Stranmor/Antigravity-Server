//! Smart warmup settings

use crate::app::AppState;
use leptos::prelude::*;

fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.checked())
        .unwrap_or(false)
}

/// Smart warmup settings section.
#[component]
pub(crate) fn SmartWarmupSettings() -> impl IntoView {
    let state = expect_context::<AppState>();

    view! {
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
    }
}
