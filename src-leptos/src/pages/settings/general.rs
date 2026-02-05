//! General settings section (language, theme, auto-launch)

use crate::app::AppState;
use leptos::prelude::*;

/// Helper to get checked state from checkbox event
fn event_target_checked(ev: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;
    ev.target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        .map(|input| input.checked())
        .unwrap_or(false)
}

/// General settings section.
#[component]
pub(crate) fn GeneralSettings() -> impl IntoView {
    let state = expect_context::<AppState>();

    view! {
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
    }
}
