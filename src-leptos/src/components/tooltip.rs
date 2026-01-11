//! Tooltip component

use leptos::prelude::*;
use leptos_use::{use_element_bounding, use_timeout_fn, use_window_size};

#[component]
pub fn Tooltip(#[prop(into)] text: String, children: Children) -> impl IntoView {
    let (is_visible, set_is_visible) = RwSignal::new(false);
    let (target_ref, set_target_ref) = create_node_ref();
    let (tooltip_ref, set_tooltip_ref) = create_node_ref();

    // Use element bounding to get position of target and tooltip
    let target_bounding = use_element_bounding(target_ref);
    let tooltip_bounding = use_element_bounding(tooltip_ref);
    let window_size = use_window_size();

    // Calculate position of the tooltip
    let tooltip_style = Memo::new(move |_| {
        let target_rect = target_bounding.get();
        let tooltip_rect = tooltip_bounding.get();
        let window = window_size.get();

        let mut top = target_rect.top + target_rect.height + 8.0; // 8px offset below target
        let mut left = target_rect.left + (target_rect.width / 2.0) - (tooltip_rect.width / 2.0);

        // Keep tooltip within window bounds (simple collision detection)
        if left < 0.0 {
            left = 0.0;
        } else if (left + tooltip_rect.width) > window.width {
            left = window.width - tooltip_rect.width;
        }

        if (top + tooltip_rect.height) > window.height && target_rect.top > tooltip_rect.height {
            // If it goes off screen below, and there's space above, position above
            top = target_rect.top - tooltip_rect.height - 8.0;
        }

        format!("top: {}px; left: {}px;", top, left)
    });

    // Debounce visibility for a smoother UX
    let show_tooltip_debounced = use_timeout_fn(
        move || set_is_visible.set(true),
        200, // Show after 200ms hover
    );

    let hide_tooltip_debounced = use_timeout_fn(
        move || set_is_visible.set(false),
        100, // Hide after 100ms mouse out
    );

    view! {
        <div
            class="relative inline-block"
            on:mouseenter=move |_| show_tooltip_debounced.start()
            on:mouseleave=move |_| {
                show_tooltip_debounced.clear(); // Clear show timeout if mouse leaves quickly
                hide_tooltip_debounced.start();
            }
            node_ref=target_ref
        >
            {children()}

            <Show when=is_visible>
                <div
                    class="absolute z-50 px-3 py-1.5 text-xs text-white bg-gray-800 dark:bg-gray-700 rounded-md shadow-sm transition-opacity duration-150 whitespace-nowrap"
                    style=tooltip_style
                    node_ref=tooltip_ref
                >
                    {text.clone()}
                </div>
            </Show>
        </div>
    }
}
