use super::actions::on_clear_bindings;
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;

/// Load balancing and scheduling configuration.
#[component]
pub(crate) fn Scheduling(
    scheduling_expanded: RwSignal<bool>,
    scheduling_mode: RwSignal<String>,
    sticky_session_ttl: RwSignal<u32>,
    message: RwSignal<Option<(String, bool)>>,
) -> impl IntoView {
    view! {
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
                        on_click=move || on_clear_bindings(message)
                    />
                </div>
            </Show>
        </div>
    }
}
