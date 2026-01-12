//! State actions module
//!
//! Provides structured action handlers that avoid the closure capture issues
//! by using cloned state references.

use crate::app::AppState;
use crate::api::commands;
use crate::types::Account;
use leptos::task::spawn_local;

/// Account-related actions
#[derive(Clone)]
pub struct AccountActions {
    state: AppState,
}

impl AccountActions {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Refresh the accounts list from backend
    pub fn refresh_list(&self) {
        let s = self.state.clone();
        spawn_local(async move {
            if let Ok(accounts) = commands::list_accounts().await {
                s.accounts.set(accounts);
            }
        });
    }

    /// Switch to a different account
    pub fn switch(&self, account_id: String, on_success: impl Fn() + 'static) {
        let s = self.state.clone();
        spawn_local(async move {
            if commands::switch_account(&account_id).await.is_ok() {
                s.current_account_id.set(Some(account_id));
                on_success();
            }
        });
    }

    /// Delete an account
    pub fn delete(&self, account_id: String, on_success: impl Fn() + 'static) {
        let s = self.state.clone();
        spawn_local(async move {
            if commands::delete_account(&account_id).await.is_ok() {
                if let Ok(accounts) = commands::list_accounts().await {
                    s.accounts.set(accounts);
                }
                on_success();
            }
        });
    }

    /// Delete multiple accounts
    pub fn delete_batch(&self, account_ids: Vec<String>, on_success: impl Fn(usize) + 'static) {
        let s = self.state.clone();
        let count = account_ids.len();
        spawn_local(async move {
            if commands::delete_accounts(&account_ids).await.is_ok() {
                if let Ok(accounts) = commands::list_accounts().await {
                    s.accounts.set(accounts);
                }
                on_success(count);
            }
        });
    }

    /// Refresh quota for a single account
    pub fn refresh_quota(&self, account_id: String, on_complete: impl Fn() + 'static) {
        let s = self.state.clone();
        spawn_local(async move {
            let _ = commands::fetch_account_quota(&account_id).await;
            if let Ok(accounts) = commands::list_accounts().await {
                s.accounts.set(accounts);
            }
            on_complete();
        });
    }

    /// Refresh all account quotas
    pub fn refresh_all_quotas(&self, on_result: impl Fn(Result<(usize, usize), String>) + 'static) {
        let s = self.state.clone();
        spawn_local(async move {
            match commands::refresh_all_quotas().await {
                Ok(stats) => {
                    if let Ok(accounts) = commands::list_accounts().await {
                        s.accounts.set(accounts);
                    }
                    on_result(Ok((stats.success, stats.total)));
                }
                Err(e) => on_result(Err(e)),
            }
        });
    }

    /// Start OAuth login
    pub fn oauth_login(&self, on_result: impl Fn(Result<Account, String>) + 'static) {
        let s = self.state.clone();
        spawn_local(async move {
            match commands::start_oauth_login().await {
                Ok(account) => {
                    if let Ok(accounts) = commands::list_accounts().await {
                        s.accounts.set(accounts);
                    }
                    on_result(Ok(account));
                }
                Err(e) => on_result(Err(e)),
            }
        });
    }

    /// Sync from local DB
    pub fn sync_from_db(&self, on_result: impl Fn(Result<Option<Account>, String>) + 'static) {
        let s = self.state.clone();
        spawn_local(async move {
            match commands::sync_account_from_db().await {
                Ok(account) => {
                    if account.is_some() {
                        if let Ok(accounts) = commands::list_accounts().await {
                            s.accounts.set(accounts);
                        }
                    }
                    on_result(Ok(account));
                }
                Err(e) => on_result(Err(e)),
            }
        });
    }
}

/// Proxy-related actions
#[derive(Clone)]
pub struct ProxyActions {
    state: AppState,
}

impl ProxyActions {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Toggle proxy on/off
    pub fn toggle(&self, on_complete: impl Fn() + 'static) {
        let s = self.state.clone();
        spawn_local(async move {
            let current = s.proxy_status.get();
            let result = if current.running {
                commands::stop_proxy_service().await
            } else {
                commands::start_proxy_service().await.map(|_| ())
            };

            if result.is_ok() {
                if let Ok(status) = commands::get_proxy_status().await {
                    s.proxy_status.set(status);
                }
            }
            on_complete();
        });
    }

    /// Refresh proxy status
    pub fn refresh_status(&self) {
        let s = self.state.clone();
        spawn_local(async move {
            if let Ok(status) = commands::get_proxy_status().await {
                s.proxy_status.set(status);
            }
        });
    }
}

/// Config-related actions
#[derive(Clone)]
pub struct ConfigActions {
    state: AppState,
}

impl ConfigActions {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Save current config
    pub fn save(&self, on_result: impl Fn(Result<(), String>) + 'static) {
        let s = self.state.clone();
        spawn_local(async move {
            if let Some(config) = s.config.get() {
                match commands::save_config(&config).await {
                    Ok(()) => on_result(Ok(())),
                    Err(e) => on_result(Err(e)),
                }
            }
        });
    }

    /// Reload config from backend
    pub fn reload(&self) {
        let s = self.state.clone();
        spawn_local(async move {
            if let Ok(config) = commands::load_config().await {
                s.config.set(Some(config));
            }
        });
    }
}
