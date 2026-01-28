//! Sidebar navigation component

use leptos::prelude::*;
use leptos_router::hooks::use_location;

const VERSION: &str = env!("GIT_VERSION");

const ICON_DASHBOARD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="9" rx="1"/><rect x="14" y="3" width="7" height="5" rx="1"/><rect x="14" y="12" width="7" height="9" rx="1"/><rect x="3" y="16" width="7" height="5" rx="1"/></svg>"#;

const ICON_ACCOUNTS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>"#;

const ICON_PROXY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22c5.523 0 10-4.477 10-10S17.523 2 12 2 2 6.477 2 12s4.477 10 10 10z"/><path d="m14.7 6.3-5.4 5.4"/><circle cx="10" cy="7" r="1.5"/><circle cx="14" cy="17" r="1.5"/><path d="m9.3 17.7 5.4-5.4"/></svg>"#;

const ICON_SETTINGS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>"#;

const ICON_MONITOR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>"#;

const ICON_LOGO: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"/><path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z"/><path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0"/><path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5"/></svg>"#;

#[component]
pub fn Sidebar() -> impl IntoView {
    let location = use_location();

    let nav_items: Vec<(&str, &str, &str)> = vec![
        ("Dashboard", "/", ICON_DASHBOARD),
        ("Accounts", "/accounts", ICON_ACCOUNTS),
        ("API Proxy", "/proxy", ICON_PROXY),
        ("Settings", "/settings", ICON_SETTINGS),
    ];

    view! {
        <aside class="sidebar">
            <div class="sidebar-header">
                <div class="logo">
                    <span class="logo-icon" inner_html=ICON_LOGO></span>
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
                            <span class="nav-icon" inner_html=icon></span>
                            <span class="nav-label">{label}</span>
                        </a>
                    }
                }).collect_view()}
            </nav>

            <div class="sidebar-footer">
                <a href="/monitor" class="nav-item">
                    <span class="nav-icon" inner_html=ICON_MONITOR></span>
                    <span class="nav-label">"Monitor"</span>
                </a>
            </div>
        </aside>
    }
}
