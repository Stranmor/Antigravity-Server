use super::actions::on_add_mapping;
use crate::api_models::ZaiDispatchMode;
use crate::components::{Button, ButtonVariant, CollapsibleCard, Select};
use leptos::prelude::*;
use std::collections::HashMap;

/// Z.ai provider configuration section.
#[component]
pub(crate) fn ZaiProvider(
    zai_expanded: RwSignal<bool>,
    zai_enabled: RwSignal<bool>,
    zai_base_url: RwSignal<String>,
    zai_api_key: RwSignal<String>,
    zai_dispatch_mode: RwSignal<ZaiDispatchMode>,
    zai_model_mapping: RwSignal<HashMap<String, String>>,
    new_mapping_from: RwSignal<String>,
    new_mapping_to: RwSignal<String>,
) -> impl IntoView {
    view! {
        <CollapsibleCard title="Z.ai Provider".to_string() initial_expanded=zai_expanded.get_untracked()>
            <div class="config-content">
                <div class="form-group form-group--toggle">
                    <label>"Enable Z.ai"</label>
                    <input
                        type="checkbox"
                        class="toggle"
                        prop:checked=move || zai_enabled.get()
                        on:change=move |ev| zai_enabled.set(event_target_checked(&ev))
                    />
                </div>

                <Show when=move || zai_enabled.get()>
                    <div class="config-grid">
                        <div class="form-group">
                            <label>"Base URL"</label>
                            <input
                                type="text"
                                placeholder="https://api.z.ai/api/anthropic"
                                prop:value=move || zai_base_url.get()
                                on:input=move |ev| zai_base_url.set(event_target_value(&ev))
                            />
                        </div>
                        <div class="form-group">
                            <label>"API Key"</label>
                            <input
                                type="password"
                                placeholder="sk-..."
                                prop:value=move || zai_api_key.get()
                                on:input=move |ev| zai_api_key.set(event_target_value(&ev))
                            />
                        </div>
                        <div class="form-group">
                            <label>"Dispatch Mode"</label>
                            <Select
                                options=Signal::derive(move || {
                                    vec![
                                        ("off".to_string(), "Off".to_string()),
                                        ("exclusive".to_string(), "Exclusive".to_string()),
                                        ("pooled".to_string(), "Pooled".to_string()),
                                        ("fallback".to_string(), "Fallback".to_string()),
                                    ]
                                })
                                value=Signal::derive(move || zai_dispatch_mode.get().to_string())
                                on_change=Callback::new(move |val: String| {
                                    let mode = match val.as_str() {
                                        "off" => ZaiDispatchMode::Off,
                                        "exclusive" => ZaiDispatchMode::Exclusive,
                                        "pooled" => ZaiDispatchMode::Pooled,
                                        "fallback" => ZaiDispatchMode::Fallback,
                                        _ => ZaiDispatchMode::Off,
                                    };
                                    zai_dispatch_mode.set(mode);
                                })
                            />
                        </div>
                    </div>

                    <h3>"Z.ai Model Mapping"</h3>
                    <div class="mapping-form">
                        <input
                            type="text"
                            placeholder="From (e.g., claude-3-opus*)"
                            prop:value=move || new_mapping_from.get()
                            on:input=move |ev| new_mapping_from.set(event_target_value(&ev))
                        />
                        <span class="arrow">"→"</span>
                        <input
                            type="text"
                            placeholder="To (e.g., glm-4.7)"
                            prop:value=move || new_mapping_to.get()
                            on:input=move |ev| new_mapping_to.set(event_target_value(&ev))
                        />
                        <Button
                            text="➕".to_string()
                            variant=ButtonVariant::Primary
                            on_click=move || on_add_mapping(new_mapping_from, new_mapping_to, zai_model_mapping)
                        />
                    </div>

                    <div class="mapping-list">
                        {move || {
                            let mappings: Vec<_> = zai_model_mapping.get().into_iter().collect();
                            if mappings.is_empty() {
                                view! { <p class="empty-text">"No Z.ai model mappings"</p> }.into_any()
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
                                                on:click=move |_| zai_model_mapping.update(|m| { m.remove(&from_clone); })
                                            >"×"</button>
                                        </div>
                                    }
                                }).collect_view().into_any()
                            }
                        }}
                    </div>
                </Show>
            </div>
        </CollapsibleCard>
    }
}
