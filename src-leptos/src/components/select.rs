//! Custom Select dropdown component

use leptos::prelude::*;

#[component]
pub fn Select(
    #[prop(into)] options: Signal<Vec<(String, String)>>,
    #[prop(into)] value: Signal<String>,
    #[prop(into)] on_change: Callback<String>,
    #[prop(into, optional)] placeholder: Option<String>,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    let is_open = RwSignal::new(false);
    let selected_label = RwSignal::new(String::new());

    Effect::new(move |_| {
        let current_value = value.get();
        let opts = options.get();
        if let Some((_, label)) = opts.iter().find(|(v, _)| v == &current_value) {
            selected_label.set(label.clone());
        } else {
            selected_label.set(placeholder.clone().unwrap_or_default());
        }
    });

    let on_select = move |new_value: String| {
        on_change.run(new_value);
        is_open.set(false);
    };

    view! {
        <div class="select-wrapper">
            <button
                class="select-button"
                on:click=move |_| is_open.update(|o| *o = !*o)
                disabled=disabled
            >
                {move || selected_label.get()}
                <span class="select-arrow">{move || if is_open.get() { "▲" } else { "▼" }}</span>
            </button>

            <Show when=move || is_open.get()>
                <div class="select-dropdown">
                    {options.get().into_iter().map(|(val, label)| {
                        let val_clone = val.clone();
                        view! {
                            <div
                                class="select-option"
                                class:selected=move || value.get() == val
                                on:click=move |_| on_select(val_clone.clone())
                            >
                                {label}
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Show>
        </div>
    }
}

