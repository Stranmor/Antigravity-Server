//! Main App component with routing

use crate::api::auth::is_authenticated;
use crate::components::Sidebar;
use crate::pages::{Accounts, ApiProxy, Dashboard, Login, Monitor, Settings};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::{
    components::{Route, Router, Routes},
    hooks::use_navigate,
    path,
};
use log;

/// Global application state shared across components.
#[derive(Clone, Copy, Debug)]
pub struct AppState {
    /// List of user accounts.
    pub accounts: RwSignal<Vec<crate::api_models::Account>>,
    /// Currently selected account ID.
    pub current_account_id: RwSignal<Option<String>>,
    /// Application configuration.
    pub config: RwSignal<Option<crate::api_models::AppConfig>>,
    /// Current proxy service status.
    pub proxy_status: RwSignal<crate::api_models::ProxyStatus>,
    /// Loading indicator state.
    pub loading: RwSignal<bool>,
}

impl AppState {
    /// Creates a new AppState with default values.
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
    view! {
        <Router>
            <Routes fallback=|| "Page not found">
                <Route path=path!("/login") view=Login />
                <Route path=path!("/") view=AuthenticatedApp />
                <Route path=path!("/accounts") view=AuthenticatedApp />
                <Route path=path!("/proxy") view=AuthenticatedApp />
                <Route path=path!("/settings") view=AuthenticatedApp />
                <Route path=path!("/monitor") view=AuthenticatedApp />
            </Routes>
        </Router>
    }
}

#[component]
fn AuthenticatedApp() -> impl IntoView {
    let navigate = use_navigate();

    if !is_authenticated() {
        navigate("/login", Default::default());
        return view! { <div>"Redirecting..."</div> }.into_any();
    }

    let state = AppState::new();
    provide_context(state);

    let _init_effect = Effect::new(move |_| {
        let s = state;
        spawn_local(async move {
            load_initial_data(s).await;
        });
    });

    view! {
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
    }
    .into_any()
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
