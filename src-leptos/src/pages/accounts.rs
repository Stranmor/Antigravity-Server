//! Accounts page with full parity

use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashSet;
use crate::app::AppState;
use crate::components::{Button, ButtonVariant, Modal, ModalType, Pagination, AccountCard};
use crate::tauri::commands;
use crate::types::Account;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ViewMode {
    #[default]
    List,
    Grid,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum FilterType {
    #[default]
    All,
    Pro,
    Ultra,
    Free,
}

#[component]
pub fn Accounts() -> impl IntoView {
    let state = expect_context::<AppState>();
    
    // View and filter state
    let view_mode = RwSignal::new(ViewMode::List);
    let filter = RwSignal::new(FilterType::All);
    let search_query = RwSignal::new(String::new());
    
    // Selection state
    let selected_ids = RwSignal::new(HashSet::<String>::new());
    
    // Pagination state
    let current_page = RwSignal::new(1usize);
    let items_per_page = RwSignal::new(20usize);
    
    // Loading states
    let refresh_pending = RwSignal::new(false);
    let oauth_pending = RwSignal::new(false);
    let sync_pending = RwSignal::new(false);
    let refreshing_ids = RwSignal::new(HashSet::<String>::new());
    
    // Modal states
    let delete_confirm = RwSignal::new(Option::<String>::None);
    let batch_delete_confirm = RwSignal::new(false);
    let toggle_proxy_confirm = RwSignal::new(Option::<(String, bool)>::None);
    
    // Messages
    let message = RwSignal::new(Option::<(String, bool)>::None);
    
    let show_message = move |msg: String, is_error: bool| {
        message.set(Some((msg, is_error)));
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            message.set(None);
        });
    };
    
    // Filter counts
    let filter_counts = Memo::new(move |_| {
        let accounts = state.accounts.get();
        let all = accounts.len();
        let pro = accounts.iter()
            .filter(|a| a.quota.as_ref()
                .and_then(|q| q.subscription_tier.as_ref())
                .is_some_and(|t| t.to_lowercase().contains("pro")))
            .count();
        let ultra = accounts.iter()
            .filter(|a| a.quota.as_ref()
                .and_then(|q| q.subscription_tier.as_ref())
                .is_some_and(|t| t.to_lowercase().contains("ultra")))
            .count();
        let free = all - pro - ultra;
        (all, pro, ultra, free)
    });
    
    // Filtered accounts
    let filtered_accounts = Memo::new(move |_| {
        let query = search_query.get().to_lowercase();
        let accounts = state.accounts.get();
        let current_filter = filter.get();
        
        accounts.into_iter()
            .filter(|a| {
                // Search filter
                if !query.is_empty() && !a.email.to_lowercase().contains(&query) {
                    return false;
                }
                // Tier filter
                match current_filter {
                    FilterType::All => true,
                    FilterType::Pro => a.quota.as_ref()
                        .and_then(|q| q.subscription_tier.as_ref())
                        .is_some_and(|t| t.to_lowercase().contains("pro")),
                    FilterType::Ultra => a.quota.as_ref()
                        .and_then(|q| q.subscription_tier.as_ref())
                        .is_some_and(|t| t.to_lowercase().contains("ultra")),
                    FilterType::Free => {
                        let tier = a.quota.as_ref()
                            .and_then(|q| q.subscription_tier.as_ref())
                            .map(|t| t.to_lowercase())
                            .unwrap_or_default();
                        !tier.contains("pro") && !tier.contains("ultra")
                    }
                }
            })
            .collect::<Vec<_>>()
    });
    
    // Paginated accounts
    let paginated_accounts = Memo::new(move |_| {
        let all = filtered_accounts.get();
        let page = current_page.get();
        let per_page = items_per_page.get();
        let start = (page - 1) * per_page;
        all.into_iter().skip(start).take(per_page).collect::<Vec<_>>()
    });
    
    let total_pages = Memo::new(move |_| {
        let total = filtered_accounts.get().len();
        let per_page = items_per_page.get();
        (total + per_page - 1) / per_page.max(1)
    });
    
    // Selection helpers
    let selected_count = Memo::new(move |_| selected_ids.get().len());
    let all_page_selected = Memo::new(move |_| {
        let page_accounts = paginated_accounts.get();
        let selected = selected_ids.get();
        !page_accounts.is_empty() && page_accounts.iter().all(|a| selected.contains(&a.id))
    });
    
    // Reset pagination when filter changes
    Effect::new(move |_| {
        let _ = filter.get();
        let _ = search_query.get();
        current_page.set(1);
        selected_ids.set(HashSet::new());
    });
    
    // Actions
    let on_refresh_list = move || {
        refresh_pending.set(true);
        spawn_local(async move {
            if let Ok(accounts) = commands::list_accounts().await {
                let state = expect_context::<AppState>();
                state.accounts.set(accounts);
            }
            refresh_pending.set(false);
        });
    };
    
    let on_refresh_all_quotas = move || {
        refresh_pending.set(true);
        spawn_local(async move {
            match commands::refresh_all_quotas().await {
                Ok(stats) => {
                    show_message(format!("Refreshed {}/{} accounts", stats.success, stats.total), false);
                    if let Ok(accounts) = commands::list_accounts().await {
                        let state = expect_context::<AppState>();
                        state.accounts.set(accounts);
                    }
                }
                Err(e) => show_message(format!("Failed: {}", e), true),
            }
            refresh_pending.set(false);
        });
    };
    
    let on_add_account = move || {
        oauth_pending.set(true);
        spawn_local(async move {
            match commands::start_oauth_login().await {
                Ok(account) => {
                    show_message(format!("Added: {}", account.email), false);
                    if let Ok(accounts) = commands::list_accounts().await {
                        let state = expect_context::<AppState>();
                        state.accounts.set(accounts);
                    }
                }
                Err(e) => show_message(format!("OAuth failed: {}", e), true),
            }
            oauth_pending.set(false);
        });
    };
    
    let on_sync_local = move || {
        sync_pending.set(true);
        spawn_local(async move {
            match commands::sync_account_from_db().await {
                Ok(Some(account)) => {
                    show_message(format!("Synced: {}", account.email), false);
                    if let Ok(accounts) = commands::list_accounts().await {
                        let state = expect_context::<AppState>();
                        state.accounts.set(accounts);
                    }
                }
                Ok(None) => show_message("No account found in local DB".to_string(), true),
                Err(e) => show_message(format!("Sync failed: {}", e), true),
            }
            sync_pending.set(false);
        });
    };
    
    let on_switch_account = move |account_id: String| {
        spawn_local(async move {
            if commands::switch_account(&account_id).await.is_ok() {
                let state = expect_context::<AppState>();
                state.current_account_id.set(Some(account_id));
                show_message("Account switched".to_string(), false);
            }
        });
    };
    
    let on_refresh_account = move |account_id: String| {
        let aid = account_id.clone();
        refreshing_ids.update(|ids| { ids.insert(aid); });
        spawn_local(async move {
            let _ = commands::fetch_account_quota(&account_id).await;
            if let Ok(accounts) = commands::list_accounts().await {
                let state = expect_context::<AppState>();
                state.accounts.set(accounts);
            }
            refreshing_ids.update(|ids| { ids.remove(&account_id); });
        });
    };
    
    let on_delete_account = move |account_id: String| {
        delete_confirm.set(Some(account_id));
    };
    
    let execute_delete = move || {
        if let Some(id) = delete_confirm.get() {
            delete_confirm.set(None);
            spawn_local(async move {
                if commands::delete_account(&id).await.is_ok() {
                    if let Ok(accounts) = commands::list_accounts().await {
                        let state = expect_context::<AppState>();
                        state.accounts.set(accounts);
                    }
                    show_message("Account deleted".to_string(), false);
                }
            });
        }
    };
    
    let on_batch_delete = move || {
        if selected_ids.get().len() > 0 {
            batch_delete_confirm.set(true);
        }
    };
    
    let execute_batch_delete = move || {
        let ids: Vec<String> = selected_ids.get().into_iter().collect();
        let count = ids.len();
        batch_delete_confirm.set(false);
        spawn_local(async move {
            if commands::delete_accounts(&ids).await.is_ok() {
                selected_ids.set(HashSet::new());
                if let Ok(accounts) = commands::list_accounts().await {
                    let state = expect_context::<AppState>();
                    state.accounts.set(accounts);
                }
                show_message(format!("Deleted {} accounts", count), false);
            }
        });
    };
    
    let on_toggle_proxy = move |account_id: String, current_disabled: bool| {
        toggle_proxy_confirm.set(Some((account_id, !current_disabled)));
    };
    
    let execute_toggle_proxy = move || {
        if let Some((id, enable)) = toggle_proxy_confirm.get() {
            toggle_proxy_confirm.set(None);
            spawn_local(async move {
                // This would need a toggle_proxy_status command
                // For now we'll just refresh the list
                show_message(format!("Proxy {} (TODO: implement)", if enable { "enabled" } else { "disabled" }), false);
            });
        }
    };
    
    let on_toggle_select = move |account_id: String| {
        selected_ids.update(|ids| {
            if ids.contains(&account_id) {
                ids.remove(&account_id);
            } else {
                ids.insert(account_id);
            }
        });
    };
    
    let on_toggle_all = move || {
        let page_ids: HashSet<String> = paginated_accounts.get().iter().map(|a| a.id.clone()).collect();
        if all_page_selected.get() {
            selected_ids.update(|ids| {
                for id in &page_ids {
                    ids.remove(id);
                }
            });
        } else {
            selected_ids.update(|ids| {
                ids.extend(page_ids);
            });
        }
    };
    
    let on_page_change = Callback::new(move |page: usize| {
        current_page.set(page);
    });
    
    let on_page_size_change = Callback::new(move |size: usize| {
        items_per_page.set(size);
        current_page.set(1);
    });

    view! {
        <div class="page accounts">
            <header class="page-header">
                <div class="header-left">
                    <h1>"Accounts"</h1>
                    <p class="subtitle">
                        {move || format!("{} accounts", state.accounts.get().len())}
                    </p>
                </div>
                <div class="header-actions">
                    <Button 
                        text="📥 Sync".to_string()
                        variant=ButtonVariant::Secondary
                        on_click=on_sync_local
                        loading=sync_pending.get()
                    />
                    <Button 
                        text="🔄 Refresh All".to_string()
                        variant=ButtonVariant::Secondary
                        on_click=on_refresh_all_quotas
                        loading=refresh_pending.get()
                    />
                    <Button 
                        text="➕ Add".to_string()
                        variant=ButtonVariant::Primary 
                        on_click=on_add_account
                        loading=oauth_pending.get()
                    />
                </div>
            </header>
            
            // Message banner
            <Show when=move || message.get().is_some()>
                {move || {
                    let (msg, is_error) = message.get().unwrap();
                    view! {
                        <div class=format!("alert {}", if is_error { "alert--error" } else { "alert--success" })>
                            <span>{msg}</span>
                        </div>
                    }
                }}
            </Show>
            
            // Toolbar
            <div class="toolbar">
                // Search
                <div class="search-box">
                    <input 
                        type="text"
                        placeholder="Search accounts..."
                        prop:value=move || search_query.get()
                        on:input=move |ev| search_query.set(event_target_value(&ev))
                    />
                </div>
                
                // View mode toggle
                <div class="view-toggle">
                    <button 
                        class=move || if matches!(view_mode.get(), ViewMode::List) { "active" } else { "" }
                        on:click=move |_| view_mode.set(ViewMode::List)
                        title="List view"
                    >"☰"</button>
                    <button 
                        class=move || if matches!(view_mode.get(), ViewMode::Grid) { "active" } else { "" }
                        on:click=move |_| view_mode.set(ViewMode::Grid)
                        title="Grid view"
                    >"▦"</button>
                </div>
                
                // Filter tabs
                <div class="filter-tabs">
                    <button 
                        class=move || if matches!(filter.get(), FilterType::All) { "active" } else { "" }
                        on:click=move |_| filter.set(FilterType::All)
                    >
                        "All"
                        <span class="filter-count">{move || filter_counts.get().0}</span>
                    </button>
                    <button 
                        class=move || if matches!(filter.get(), FilterType::Pro) { "active" } else { "" }
                        on:click=move |_| filter.set(FilterType::Pro)
                    >
                        "Pro"
                        <span class="filter-count">{move || filter_counts.get().1}</span>
                    </button>
                    <button 
                        class=move || if matches!(filter.get(), FilterType::Ultra) { "active" } else { "" }
                        on:click=move |_| filter.set(FilterType::Ultra)
                    >
                        "Ultra"
                        <span class="filter-count">{move || filter_counts.get().2}</span>
                    </button>
                    <button 
                        class=move || if matches!(filter.get(), FilterType::Free) { "active" } else { "" }
                        on:click=move |_| filter.set(FilterType::Free)
                    >
                        "Free"
                        <span class="filter-count">{move || filter_counts.get().3}</span>
                    </button>
                </div>
                
                <div class="toolbar-spacer"></div>
                
                // Selection actions
                <Show when=move || selected_count.get() != 0>
                    <div class="selection-actions">
                        <span class="selection-count">{move || selected_count.get()}" selected"</span>
                        <Button 
                            text="🗑 Delete".to_string()
                            variant=ButtonVariant::Danger
                            on_click=on_batch_delete
                        />
                    </div>
                </Show>
            </div>
            
            // Content
            <Show 
                when=move || matches!(view_mode.get(), ViewMode::List)
                fallback=move || view! {
                    // Grid view
                    <div class="accounts-grid">
                        <For
                            each=move || paginated_accounts.get()
                            key=|a| a.id.clone()
                            children=move |account| {
                                let id = account.id.clone();
                                let id2 = account.id.clone();
                                let id3 = account.id.clone();
                                let id4 = account.id.clone();
                                let id5 = account.id.clone();
                                let id6 = account.id.clone();
                                let proxy_disabled = account.proxy_disabled;
                                
                                view! {
                                    <AccountCard
                                        account=account.clone()
                                        is_current=Signal::derive(move || state.current_account_id.get() == Some(id.clone()))
                                        is_selected=Signal::derive(move || selected_ids.get().contains(&id2))
                                        is_refreshing=Signal::derive(move || refreshing_ids.get().contains(&id3))
                                        on_select=Callback::new(move |_| on_toggle_select(id4.clone()))
                                        on_switch=Callback::new(move |_| on_switch_account(id5.clone()))
                                        on_refresh=Callback::new(move |_| on_refresh_account(id6.clone()))
                                        on_delete=Callback::new({
                                            let id = account.id.clone();
                                            move |_| on_delete_account(id.clone())
                                        })
                                        on_toggle_proxy=Callback::new({
                                            let id = account.id.clone();
                                            move |_| on_toggle_proxy(id.clone(), proxy_disabled)
                                        })
                                    />
                                }
                            }
                        />
                    </div>
                }
            >
                // List view
                <div class="accounts-table-container">
                    <table class="accounts-table">
                        <thead>
                            <tr>
                                <th class="col-checkbox">
                                    <input 
                                        type="checkbox"
                                        checked=move || all_page_selected.get()
                                        on:change=move |_| on_toggle_all()
                                    />
                                </th>
                                <th class="col-status"></th>
                                <th class="col-email">"Email"</th>
                                <th class="col-tier">"Tier"</th>
                                <th class="col-quota">"Gemini"</th>
                                <th class="col-quota">"Claude"</th>
                                <th class="col-proxy">"Proxy"</th>
                                <th class="col-actions">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || paginated_accounts.get()
                                key=|account| account.id.clone()
                                children=move |account| {
                                    let account_id = account.id.clone();
                                    let account_id2 = account.id.clone();
                                    let account_id3 = account.id.clone();
                                    let account_id4 = account.id.clone();
                                    let account_id5 = account.id.clone();
                                    let account_id_select = account.id.clone();
                                    let account_id_switch = account.id.clone();
                                    let account_id_refresh = account.id.clone();
                                    let account_id_delete = account.id.clone();
                                    let account_id_proxy = account.id.clone();
                                    let email = account.email.clone();
                                    let is_disabled = account.disabled;
                                    let proxy_disabled = account.proxy_disabled;
                                    
                                    let tier = account.quota.as_ref()
                                        .and_then(|q| q.subscription_tier.clone())
                                        .unwrap_or_else(|| "Free".to_string());
                                    
                                    let tier_class = if tier.to_lowercase().contains("ultra") {
                                        "tier-ultra"
                                    } else if tier.to_lowercase().contains("pro") {
                                        "tier-pro"
                                    } else {
                                        "tier-free"
                                    };
                                    
                                    let quota_gemini = account.quota.as_ref().map(|q| {
                                        q.models.iter()
                                            .find(|m| m.model.contains("gemini") || m.model.contains("flash"))
                                            .map(|m| if m.limit > 0 { (m.limit - m.used) * 100 / m.limit } else { 0 })
                                            .unwrap_or(0)
                                    }).unwrap_or(0);
                                    
                                    let quota_claude = account.quota.as_ref().map(|q| {
                                        q.models.iter()
                                            .find(|m| m.model.contains("claude"))
                                            .map(|m| if m.limit > 0 { (m.limit - m.used) * 100 / m.limit } else { 0 })
                                            .unwrap_or(0)
                                    }).unwrap_or(0);
                                    
                                    let is_current = move || state.current_account_id.get() == Some(account_id.clone());
                                    let is_selected = move || selected_ids.get().contains(&account_id2);
                                    let is_refreshing = move || refreshing_ids.get().contains(&account_id3);
                                    
                                    let status_class = move || {
                                        if state.current_account_id.get() == Some(account_id4.clone()) {
                                            "status-dot--active"
                                        } else if is_disabled {
                                            "status-dot--disabled"
                                        } else {
                                            "status-dot--idle"
                                        }
                                    };
                                    
                                    view! {
                                        <tr class=move || format!(
                                            "account-row {} {}",
                                            if is_current() { "is-current" } else { "" },
                                            if is_selected() { "is-selected" } else { "" }
                                        )>
                                            <td class="col-checkbox">
                                                <input 
                                                    type="checkbox" 
                                                    checked=is_selected
                                                    on:change={
                                                        let id = account_id_select.clone();
                                                        move |_| on_toggle_select(id.clone())
                                                    }
                                                />
                                            </td>
                                            <td class="col-status">
                                                <span class=move || format!("status-dot {}", status_class())></span>
                                            </td>
                                            <td class="col-email">
                                                <span class="email-text">{email.clone()}</span>
                                                <Show when=is_current>
                                                    <span class="current-badge">"ACTIVE"</span>
                                                </Show>
                                            </td>
                                            <td class="col-tier">
                                                <span class=format!("tier-badge {}", tier_class)>{tier}</span>
                                            </td>
                                            <td class="col-quota">
                                                <div class="quota-bar">
                                                    <div 
                                                        class=format!("quota-fill {}", quota_class(quota_gemini))
                                                        style=format!("width: {}%", quota_gemini)
                                                    ></div>
                                                </div>
                                                <span class="quota-text">{quota_gemini}"%"</span>
                                            </td>
                                            <td class="col-quota">
                                                <div class="quota-bar">
                                                    <div 
                                                        class=format!("quota-fill {}", quota_class(quota_claude))
                                                        style=format!("width: {}%", quota_claude)
                                                    ></div>
                                                </div>
                                                <span class="quota-text">{quota_claude}"%"</span>
                                            </td>
                                            <td class="col-proxy">
                                                <button 
                                                    class=format!("proxy-badge {}", if proxy_disabled { "off" } else { "on" })
                                                    on:click={
                                                        let id = account_id_proxy.clone();
                                                        move |_| on_toggle_proxy(id.clone(), proxy_disabled)
                                                    }
                                                >
                                                    {if proxy_disabled { "OFF" } else { "ON" }}
                                                </button>
                                            </td>
                                            <td class="col-actions">
                                                <button 
                                                    class="btn btn--icon" 
                                                    title="Switch"
                                                    on:click={
                                                        let id = account_id_switch.clone();
                                                        move |_| on_switch_account(id.clone())
                                                    }
                                                >"⚡"</button>
                                                <button 
                                                    class=move || format!("btn btn--icon {}", if is_refreshing() { "loading" } else { "" })
                                                    title="Refresh"
                                                    disabled=is_refreshing
                                                    on:click={
                                                        let id = account_id_refresh.clone();
                                                        move |_| on_refresh_account(id.clone())
                                                    }
                                                >"🔄"</button>
                                                <button 
                                                    class="btn btn--icon btn--danger" 
                                                    title="Delete"
                                                    on:click={
                                                        let id = account_id_delete.clone();
                                                        move |_| on_delete_account(id.clone())
                                                    }
                                                >"🗑"</button>
                                            </td>
                                        </tr>
                                    }
                                }
                            />
                        </tbody>
                    </table>
                    
                    <Show when=move || paginated_accounts.get().is_empty()>
                        <div class="empty-state">
                            <span class="empty-icon">"👥"</span>
                            <p>"No accounts found"</p>
                        </div>
                    </Show>
                </div>
            </Show>
            
            // Pagination
            <Show when=move || total_pages.get() > 1>
                <Pagination
                    current_page=Signal::derive(move || current_page.get())
                    total_pages=Signal::derive(move || total_pages.get())
                    total_items=Signal::derive(move || filtered_accounts.get().len())
                    items_per_page=Signal::derive(move || items_per_page.get())
                    on_page_change=on_page_change
                    on_page_size_change=on_page_size_change
                />
            </Show>
            
            // Modals
            <Modal
                is_open=Signal::derive(move || delete_confirm.get().is_some())
                title="Delete Account".to_string()
                message=Some("Are you sure you want to delete this account?".to_string())
                modal_type=ModalType::Danger
                confirm_text=Some("Delete".to_string())
                on_confirm=Callback::new(move |_| execute_delete())
                on_cancel=Callback::new(move |_| delete_confirm.set(None))
            />
            
            <Modal
                is_open=Signal::derive(move || batch_delete_confirm.get())
                title="Delete Selected Accounts".to_string()
                message=Some(format!("Delete {} selected accounts?", selected_count.get()))
                modal_type=ModalType::Danger
                confirm_text=Some("Delete All".to_string())
                on_confirm=Callback::new(move |_| execute_batch_delete())
                on_cancel=Callback::new(move |_| batch_delete_confirm.set(false))
            />
            
            <Modal
                is_open=Signal::derive(move || toggle_proxy_confirm.get().is_some())
                title="Toggle Proxy".to_string()
                message=Some("Toggle proxy status for this account?".to_string())
                modal_type=ModalType::Confirm
                on_confirm=Callback::new(move |_| execute_toggle_proxy())
                on_cancel=Callback::new(move |_| toggle_proxy_confirm.set(None))
            />
        </div>
    }
}

fn quota_class(percent: i32) -> &'static str {
    match percent {
        0..=20 => "quota-fill--critical",
        21..=50 => "quota-fill--warning",
        _ => "quota-fill--good",
    }
}
