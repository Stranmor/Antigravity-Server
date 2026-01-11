//! Main App component with routing

use crate::components::Sidebar;
use crate::pages::{Accounts, ApiProxy, Dashboard, Monitor, Settings};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

/// Global application state
#[derive(Clone)]
pub struct AppState {
    pub accounts: RwSignal<Vec<crate::types::Account>>,
    pub current_account_id: RwSignal<Option<String>>,
    pub config: RwSignal<Option<crate::types::AppConfig>>,
    pub proxy_status: RwSignal<crate::types::ProxyStatus>,
    pub loading: RwSignal<bool>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            accounts: RwSignal::new(vec![]),
            current_account_id: RwSignal::new(None),
            config: RwSignal::new(None),
            proxy_status: RwSignal::new(Default::default()),
            loading: RwSignal::new(false),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Root App component
#[component]
pub fn App() -> impl IntoView {
    // Create global state
    let state = AppState::new();
    provide_context(state.clone());

    // Load initial data
    Effect::new(move |_| {
        spawn_local(async move {
            load_initial_data().await;
        });
    });

    view! {
        <Router>
            <div class="app-container">
                <Sidebar />
                <main class="main-content">
                    <Routes fallback=|| "Page not found">
                        <Route path=path!("/") view=Dashboard />
                        <Route path=path!("/accounts") view=Accounts />
                        <Route path=path!("/proxy") view=ApiProxy />
                        <Route path=path!("/settings") view=Settings />
                        <Route path=path!("/monitor") view=Monitor />
                    </Routes>
                </main>
            </div>
        </Router>
    }
}

/// Load initial application data from Tauri backend
async fn load_initial_data() {
    let state = expect_context::<AppState>();
    state.loading.set(true);

    // Load accounts
    if let Ok(accounts) = crate::tauri::commands::list_accounts().await {
        state.accounts.set(accounts);
    }

    // Load current account
    if let Ok(Some(account)) = crate::tauri::commands::get_current_account().await {
        state.current_account_id.set(Some(account.id));
    }

    // Load config
    if let Ok(config) = crate::tauri::commands::load_config().await {
        state.config.set(Some(config));
    }

    // Load proxy status
    if let Ok(status) = crate::tauri::commands::get_proxy_status().await {
        state.proxy_status.set(status);
    }

    state.loading.set(false);
    log::info!("Initial data loaded");
}
