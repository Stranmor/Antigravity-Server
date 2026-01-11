//! Button component with variants

use leptos::prelude::*;

#[derive(Clone, Copy, Default, PartialEq)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Danger,
    Ghost,
}

impl ButtonVariant {
    pub fn class(&self) -> &'static str {
        match self {
            ButtonVariant::Primary => "btn--primary",
            ButtonVariant::Secondary => "btn--secondary",
            ButtonVariant::Danger => "btn--danger",
            ButtonVariant::Ghost => "btn--ghost",
        }
    }
}

#[component]
pub fn Button(
    /// Button text content
    #[prop(into)]
    text: String,
    /// Button variant
    #[prop(optional)]
    variant: ButtonVariant,
    /// Whether button is disabled
    #[prop(optional)]
    disabled: bool,
    /// Whether button is in loading state
    #[prop(optional)]
    loading: bool,
    /// Additional CSS class
    #[prop(optional, into)]
    class: String,
    /// Click handler
    on_click: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let variant_class = variant.class();

    view! {
        <button
            class=move || {
                let loading_class = if loading { "btn--loading" } else { "" };
                format!("btn {} {} {}", variant_class, loading_class, class)
            }
            disabled=move || disabled || loading
            on:click=move |_| on_click()
        >
            {move || if loading { "Loading...".to_string() } else { text.clone() }}
        </button>
    }
}
