//! Sidebar navigation component

use leptos::prelude::*;
use leptos_router::hooks::use_location;

const VERSION: &str = env!("GIT_VERSION");

#[component]
pub fn Sidebar() -> impl IntoView {
    let location = use_location();

    let nav_items = vec![
        ("Dashboard", "/", "📊"),
        ("Accounts", "/accounts", "👥"),
        ("API Proxy", "/proxy", "🔌"),
        ("Settings", "/settings", "⚙️"),
    ];

    view! {
        <aside class="sidebar">
            <div class="sidebar-header">
                <div class="logo">
                    <span class="logo-icon">"🚀"</span>
                    <span class="logo-text">"Antigravity"</span>
                </div>
                <span class="version">{format!("v{}", VERSION)}</span>
            </div>

            <nav class="sidebar-nav">
                {nav_items.into_iter().map(|(label, path, icon)| {
                    let current_path = location.pathname;
                    let is_active = move || {
                        let curr = current_path.get();
                        if path == "/" {
                            curr == "/"
                        } else {
                            curr.starts_with(path)
                        }
                    };

                    view! {
                        <a
                            href=path
                            class=move || format!("nav-item {}", if is_active() { "active" } else { "" })
                        >
                            <span class="nav-icon">{icon}</span>
                            <span class="nav-label">{label}</span>
                        </a>
                    }
                }).collect_view()}
            </nav>

            <div class="sidebar-footer">
                <a href="/monitor" class="nav-item">
                    <span class="nav-icon">"📡"</span>
                    <span class="nav-label">"Monitor"</span>
                </a>
            </div>
        </aside>
    }
}
