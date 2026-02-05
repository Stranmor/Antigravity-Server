use super::actions::on_test_mapping;
use super::state::ProxyState;
use leptos::prelude::*;

/// Test mapping section for verifying model routing.
#[component]
pub(crate) fn TestMapping(ps: ProxyState) -> impl IntoView {
    let test_mapping_expanded = ps.test_mapping_expanded;
    let test_model_input = ps.test_model_input;
    let test_loading = ps.test_loading;
    let test_result = ps.test_result;
    let ps_clone = ps.clone();

    view! {
        <div class="config-card collapsible">
            <div class="config-header clickable" on:click=move |_| test_mapping_expanded.update(|v| *v = !*v)>
                <h2>"🧪 Test Mapping"</h2>
                <span class=move || format!("expand-icon {}", if test_mapping_expanded.get() { "expanded" } else { "" })>"▼"</span>
            </div>

            <Show when=move || test_mapping_expanded.get()>
                <div class="config-content">
                    <div class="form-group">
                        <label>"Enter Model Name"</label>
                        <div class="api-key-row">
                            <input
                                type="text"
                                placeholder="e.g. gpt-4-turbo"
                                prop:value=move || test_model_input.get()
                                on:input=move |ev| test_model_input.set(event_target_value(&ev))
                                on:keydown={
                                    let ps = ps_clone.clone();
                                    move |ev| {
                                        if ev.key() == "Enter" {
                                            on_test_mapping(ps.clone());
                                        }
                                    }
                                }
                                class="api-key-input"
                            />
                            <button
                                class="btn btn--primary"
                                disabled=move || test_loading.get()
                                on:click={
                                    let ps = ps.clone();
                                    move |_| on_test_mapping(ps.clone())
                                }
                            >
                                {move || if test_loading.get() { "...".to_string() } else { "Test".to_string() }}
                            </button>
                        </div>
                    </div>

                    <Show when=move || test_result.get().is_some()>
                        {move || {
                            let Some(res) = test_result.get() else {
                                return view! { <div></div> }.into_any();
                            };
                            view! {
                                <div class="config-section" style="margin-top: 1rem; padding: 1rem; background: var(--bg-secondary); border-radius: 8px;">
                                    <div class="result-row" style="margin-bottom: 0.5rem; display: flex; justify-content: space-between;">
                                        <span class="label" style="opacity: 0.7;">"Mapped To:"</span>
                                        <span class="value code" style="font-family: monospace; font-weight: bold; color: var(--accent-primary);">{res.mapped_model}</span>
                                    </div>
                                    <div class="result-row" style="display: flex; justify-content: space-between;">
                                        <span class="label" style="opacity: 0.7;">"Reason:"</span>
                                        <span class="value">{res.mapping_reason}</span>
                                    </div>
                                </div>
                            }.into_any()
                        }}
                    </Show>
                </div>
            </Show>
        </div>
    }
}
