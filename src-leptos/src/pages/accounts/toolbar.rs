use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;

use super::filter_types::{FilterType, ViewMode};

/// Toolbar with search, view toggle, and filter tabs.
#[component]
pub(crate) fn Toolbar(
    search_query: RwSignal<String>,
    view_mode: RwSignal<ViewMode>,
    filter: RwSignal<FilterType>,
    filter_counts: Memo<(usize, usize, usize, usize, usize)>,
    selected_count: Memo<usize>,
    on_batch_delete: impl Fn() + Send + Sync + 'static + Clone,
) -> impl IntoView {
    view! {
        <div class="toolbar">
            <div class="search-box">
                <input
                    type="text"
                    placeholder="Search accounts..."
                    prop:value=move || search_query.get()
                    on:input=move |ev| search_query.set(event_target_value(&ev))
                />
            </div>

            <div class="view-toggle">
                <button
                    class=move || if matches!(view_mode.get(), ViewMode::List) { "active" } else { "" }
                    on:click=move |_| view_mode.set(ViewMode::List)
                    title="List view"
                >"☰"</button>
                <button
                    class=move || if matches!(view_mode.get(), ViewMode::Grid) { "active" } else { "" }
                    on:click=move |_| view_mode.set(ViewMode::Grid)
                    title="Grid view"
                >"▦"</button>
            </div>

            <div class="filter-tabs">
                <button
                    class=move || if matches!(filter.get(), FilterType::All) { "active" } else { "" }
                    on:click=move |_| filter.set(FilterType::All)
                >
                    "All"
                    <span class="filter-count">{move || filter_counts.get().0}</span>
                </button>
                <button
                    class=move || if matches!(filter.get(), FilterType::Pro) { "active" } else { "" }
                    on:click=move |_| filter.set(FilterType::Pro)
                >
                    "Pro"
                    <span class="filter-count">{move || filter_counts.get().1}</span>
                </button>
                <button
                    class=move || if matches!(filter.get(), FilterType::Ultra) { "active" } else { "" }
                    on:click=move |_| filter.set(FilterType::Ultra)
                >
                    "Ultra"
                    <span class="filter-count">{move || filter_counts.get().2}</span>
                </button>
                <button
                    class=move || if matches!(filter.get(), FilterType::Free) { "active" } else { "" }
                    on:click=move |_| filter.set(FilterType::Free)
                >
                    "Free"
                    <span class="filter-count">{move || filter_counts.get().3}</span>
                </button>
                <button
                    class=move || if matches!(filter.get(), FilterType::NeedsVerification) { "active filter-warning" } else { "filter-warning" }
                    on:click=move |_| filter.set(FilterType::NeedsVerification)
                >
                    "⚠️ Verify"
                    <span class="filter-count">{move || filter_counts.get().4}</span>
                </button>
            </div>

            <div class="toolbar-spacer"></div>

            <Show when=move || selected_count.get() != 0>
                {
                    let on_batch_delete = on_batch_delete.clone();
                    view! {
                        <div class="selection-actions">
                            <span class="selection-count">{move || selected_count.get()}" selected"</span>
                            <Button
                                text="🗑 Delete".to_string()
                                variant=ButtonVariant::Danger
                                on_click=on_batch_delete
                            />
                        </div>
                    }
                }
            </Show>
        </div>
    }
}
