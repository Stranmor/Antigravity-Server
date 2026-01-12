//! Main App component with routing

use crate::components::Sidebar;
use crate::pages::{Accounts, ApiProxy, Dashboard, Monitor, Settings};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};
use log;

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
    let init_state = state.clone();
    Effect::new(move |_| {
        let s = init_state.clone();
        spawn_local(async move {
            load_initial_data(s).await;
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
async fn load_initial_data(state: AppState) {
    state.loading.set(true);

    // Load accounts
    match crate::api::commands::list_accounts().await {
        Ok(accounts) => state.accounts.set(accounts),
        Err(e) => log::error!("Failed to load accounts: {:?}", e),
    }

    // Load current account
    match crate::api::commands::get_current_account().await {
        Ok(Some(account)) => state.current_account_id.set(Some(account.id)),
        Ok(None) => log::info!("No current account found."),
        Err(e) => log::error!("Failed to load current account: {:?}", e),
    }

    // Load config
    match crate::api::commands::load_config().await {
        Ok(config) => state.config.set(Some(config)),
        Err(e) => log::error!("Failed to load config: {:?}", e),
    }

    // Load proxy status
    match crate::api::commands::get_proxy_status().await {
        Ok(status) => state.proxy_status.set(status),
        Err(e) => log::error!("Failed to load proxy status: {:?}", e),
    }

    state.loading.set(false);
    log::info!("Initial data loaded");
}
