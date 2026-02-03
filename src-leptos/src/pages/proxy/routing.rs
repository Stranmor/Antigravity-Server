use super::actions::{on_add_mapping, on_apply_presets, show_message};
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;
use std::collections::HashMap;

#[component]
pub fn ModelRouting(
    routing_expanded: RwSignal<bool>,
    custom_mappings: RwSignal<HashMap<String, String>>,
    new_mapping_from: RwSignal<String>,
    new_mapping_to: RwSignal<String>,
    message: RwSignal<Option<(String, bool)>>,
) -> impl IntoView {
    let on_remove_mapping = move |key: String| {
        custom_mappings.update(|m| {
            m.remove(&key);
        });
    };

    let on_reset_mappings = move || {
        custom_mappings.set(HashMap::new());
        show_message(message, "Mappings reset".to_string(), false);
    };

    view! {
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
                            on_click=move || on_apply_presets(custom_mappings, message)
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
                            on_click=move || on_add_mapping(new_mapping_from, new_mapping_to, custom_mappings)
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
    }
}
