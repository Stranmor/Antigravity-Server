//! Tooltip component
//!
//! A simple tooltip that appears on hover using native HTML title attribute.

use leptos::prelude::*;

#[component]
pub fn Tooltip(
    /// The text to display in the tooltip
    #[prop(into)]
    text: String,
    /// The content to wrap
    children: Children,
) -> impl IntoView {
    view! {
        <span class="tooltip-wrapper" title=text>
            {children()}
        </span>
    }
}
