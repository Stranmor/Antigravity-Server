//! Collapsible Card component

use leptos::prelude::*;

#[component]
pub fn CollapsibleCard(
    #[prop(into)] title: String,
    children: ChildrenFn,
    #[prop(default = true)] initial_expanded: bool,
) -> impl IntoView {
    let expanded = RwSignal::new(initial_expanded);

    view! {
        <div class="collapsible-card">
            <div class="collapsible-header" on:click=move |_| expanded.update(|e| *e = !*e)>
                <h3>{title.clone()}</h3>
                <span class=move || format!("expand-icon {}", if expanded.get() { "expanded" } else { "" })>
                    {move || if expanded.get() { "▲" } else { "▼" }}
                </span>
            </div>
            <Show when=move || expanded.get()>
                <div class="collapsible-content">
                    {children()}
                </div>
            </Show>
        </div>
    }
}

