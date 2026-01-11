//! Stats card component for dashboard

use leptos::prelude::*;

#[component]
pub fn StatsCard(
    #[prop(into)] title: String,
    #[prop(into)] value: Signal<String>,
    #[prop(into)] icon: String,
    #[prop(optional, into)] color: String,
    #[prop(optional)] subtitle: Option<Signal<String>>,
) -> impl IntoView {
    let color_class = if color.is_empty() { "blue".to_string() } else { color };
    
    view! {
        <div class=format!("stats-card stats-card--{}", color_class)>
            <div class="stats-card__icon">{icon}</div>
            <div class="stats-card__content">
                <div class="stats-card__value">{move || value.get()}</div>
                <div class="stats-card__title">{title}</div>
                {subtitle.map(|s| view! {
                    <div class="stats-card__subtitle">{move || s.get()}</div>
                })}
            </div>
        </div>
    }
}
