//! Action handlers for accounts page

use crate::api::commands;
use crate::app::AppState;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashSet;

#[derive(Clone)]
pub struct AccountActions {
    pub state: AppState,
    pub refresh_pending: RwSignal<bool>,
    pub sync_pending: RwSignal<bool>,
    pub refreshing_ids: RwSignal<HashSet<String>>,
    pub warmup_pending: RwSignal<bool>,
    pub delete_confirm: RwSignal<Option<String>>,
    pub batch_delete_confirm: RwSignal<bool>,
    pub toggle_proxy_confirm: RwSignal<Option<(String, bool)>>,
    pub warmup_confirm: RwSignal<bool>,
    pub selected_ids: RwSignal<HashSet<String>>,
    pub message: RwSignal<Option<(String, bool)>>,
}

impl AccountActions {
    #[allow(dead_code)]
    fn show_message(&self, msg: String, is_error: bool) {
        let message = self.message;
        message.set(Some((msg, is_error)));
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            message.set(None);
        });
    }

    pub fn on_refresh_all_quotas(&self) {
        self.refresh_pending.set(true);
        let s = self.state.clone();
        let message = self.message;
        let refresh_pending = self.refresh_pending;
        spawn_local(async move {
            match commands::refresh_all_quotas().await {
                Ok(stats) => {
                    let msg = format!("Refreshed {}/{} accounts", stats.success, stats.total);
                    message.set(Some((msg, false)));
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        message.set(None);
                    });
                    if let Ok(accounts) = commands::list_accounts().await {
                        s.accounts.set(accounts);
                    }
                }
                Err(e) => {
                    let msg = format!("Failed: {}", e);
                    message.set(Some((msg, true)));
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        message.set(None);
                    });
                }
            }
            refresh_pending.set(false);
        });
    }

    pub fn on_sync_local(&self) {
        self.sync_pending.set(true);
        let s = self.state.clone();
        let message = self.message;
        let sync_pending = self.sync_pending;
        spawn_local(async move {
            match commands::sync_account_from_db().await {
                Ok(Some(account)) => {
                    let msg = format!("Synced: {}", account.email);
                    message.set(Some((msg, false)));
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        message.set(None);
                    });
                    if let Ok(accounts) = commands::list_accounts().await {
                        s.accounts.set(accounts);
                    }
                }
                Ok(None) => {
                    message.set(Some(("No account found in local DB".to_string(), true)));
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        message.set(None);
                    });
                }
                Err(e) => {
                    let msg = format!("Sync failed: {}", e);
                    message.set(Some((msg, true)));
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        message.set(None);
                    });
                }
            }
            sync_pending.set(false);
        });
    }

    pub fn create_switch_callback(&self) -> Callback<String> {
        let s = self.state.clone();
        Callback::new(move |account_id: String| {
            let s = s.clone();
            spawn_local(async move {
                if commands::switch_account(&account_id).await.is_ok() {
                    s.current_account_id.set(Some(account_id));
                }
            });
        })
    }

    pub fn create_refresh_callback(&self) -> Callback<String> {
        let s = self.state.clone();
        let refreshing_ids = self.refreshing_ids;
        Callback::new(move |account_id: String| {
            let aid = account_id.clone();
            refreshing_ids.update(|ids| {
                ids.insert(aid);
            });
            let s = s.clone();
            spawn_local(async move {
                let _ = commands::fetch_account_quota(&account_id).await;
                if let Ok(accounts) = commands::list_accounts().await {
                    s.accounts.set(accounts);
                }
                refreshing_ids.update(|ids| {
                    ids.remove(&account_id);
                });
            });
        })
    }

    pub fn create_warmup_callback(&self) -> Callback<String> {
        let s = self.state.clone();
        let refreshing_ids = self.refreshing_ids;
        let message = self.message;
        Callback::new(move |account_id: String| {
            let aid = account_id.clone();
            refreshing_ids.update(|ids| {
                ids.insert(aid);
            });
            let s = s.clone();
            spawn_local(async move {
                match commands::warmup_account(&account_id).await {
                    Ok(msg) => {
                        message.set(Some((msg, false)));
                        spawn_local(async move {
                            gloo_timers::future::TimeoutFuture::new(3000).await;
                            message.set(None);
                        });
                    }
                    Err(e) => {
                        let msg = format!("Warmup failed: {}", e);
                        message.set(Some((msg, true)));
                        spawn_local(async move {
                            gloo_timers::future::TimeoutFuture::new(3000).await;
                            message.set(None);
                        });
                    }
                }
                refreshing_ids.update(|ids| {
                    ids.remove(&account_id);
                });
                if let Ok(accounts) = commands::list_accounts().await {
                    s.accounts.set(accounts);
                }
            });
        })
    }

    pub fn execute_delete(&self) {
        if let Some(id) = self.delete_confirm.get() {
            self.delete_confirm.set(None);
            let s = self.state.clone();
            let message = self.message;
            spawn_local(async move {
                if commands::delete_account(&id).await.is_ok() {
                    if let Ok(accounts) = commands::list_accounts().await {
                        s.accounts.set(accounts);
                    }
                    message.set(Some(("Account deleted".to_string(), false)));
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        message.set(None);
                    });
                }
            });
        }
    }

    pub fn execute_batch_delete(&self) {
        let ids: Vec<String> = self.selected_ids.get().into_iter().collect();
        let count = ids.len();
        self.batch_delete_confirm.set(false);
        let s = self.state.clone();
        let selected_ids = self.selected_ids;
        let message = self.message;
        spawn_local(async move {
            if commands::delete_accounts(&ids).await.is_ok() {
                selected_ids.set(HashSet::new());
                if let Ok(accounts) = commands::list_accounts().await {
                    s.accounts.set(accounts);
                }
                let msg = format!("Deleted {} accounts", count);
                message.set(Some((msg, false)));
                spawn_local(async move {
                    gloo_timers::future::TimeoutFuture::new(3000).await;
                    message.set(None);
                });
            }
        });
    }

    pub fn execute_toggle_proxy(&self) {
        if let Some((account_id, enable)) = self.toggle_proxy_confirm.get() {
            self.toggle_proxy_confirm.set(None);
            let s = self.state.clone();
            let message = self.message;
            spawn_local(async move {
                let reason = if enable {
                    None
                } else {
                    Some("Manually disabled")
                };
                match commands::toggle_proxy_status(&account_id, enable, reason).await {
                    Ok(()) => {
                        if let Ok(accounts) = commands::list_accounts().await {
                            s.accounts.set(accounts);
                        }
                        let msg = format!("Proxy {}", if enable { "enabled" } else { "disabled" });
                        message.set(Some((msg, false)));
                        spawn_local(async move {
                            gloo_timers::future::TimeoutFuture::new(3000).await;
                            message.set(None);
                        });
                    }
                    Err(e) => {
                        let msg = format!("Failed: {}", e);
                        message.set(Some((msg, true)));
                        spawn_local(async move {
                            gloo_timers::future::TimeoutFuture::new(3000).await;
                            message.set(None);
                        });
                    }
                }
            });
        }
    }

    pub fn on_warmup_all(&self) {
        self.warmup_confirm.set(false);
        self.warmup_pending.set(true);
        let s = self.state.clone();
        let warmup_pending = self.warmup_pending;
        let message = self.message;
        spawn_local(async move {
            match commands::warmup_all_accounts().await {
                Ok(msg) => {
                    message.set(Some((msg, false)));
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        message.set(None);
                    });
                }
                Err(e) => {
                    let msg = format!("Warmup failed: {}", e);
                    message.set(Some((msg, true)));
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(3000).await;
                        message.set(None);
                    });
                }
            }
            warmup_pending.set(false);
            if let Ok(accounts) = commands::list_accounts().await {
                s.accounts.set(accounts);
            }
        });
    }
}
