mod account_row;
mod actions;
mod content;
mod filter_types;
mod header;
mod modals;
mod toolbar;

use crate::api_models::Account;
use crate::app::AppState;
use crate::components::Pagination;
use leptos::prelude::*;
use std::collections::HashSet;

use actions::AccountActions;
use content::AccountsContent;
use filter_types::{is_pro_tier, is_ultra_tier, needs_phone_verification};
pub use filter_types::{FilterType, ViewMode};
use header::{Header, MessageBanner};
use modals::Modals;
use toolbar::Toolbar;

#[component]
pub fn Accounts() -> impl IntoView {
    let state = expect_context::<AppState>();

    let view_mode = RwSignal::new(ViewMode::List);
    let filter = RwSignal::new(FilterType::All);
    let search_query = RwSignal::new(String::new());
    let selected_ids = RwSignal::new(HashSet::<String>::new());
    let current_page = RwSignal::new(1usize);
    let items_per_page = RwSignal::new(20usize);
    let refresh_pending = RwSignal::new(false);
    let _oauth_pending = RwSignal::new(false);
    let sync_pending = RwSignal::new(false);
    let refreshing_ids = RwSignal::new(HashSet::<String>::new());
    let warmup_pending = RwSignal::new(false);
    let delete_confirm = RwSignal::new(Option::<String>::None);
    let batch_delete_confirm = RwSignal::new(false);
    let add_account_modal_open = RwSignal::new(false);
    let toggle_proxy_confirm = RwSignal::new(Option::<(String, bool)>::None);
    let details_account = RwSignal::new(Option::<Account>::None);
    let warmup_confirm = RwSignal::new(false);
    let message = RwSignal::new(Option::<(String, bool)>::None);

    let actions = AccountActions {
        state: state.clone(),
        refresh_pending,
        sync_pending,
        refreshing_ids,
        warmup_pending,
        delete_confirm,
        batch_delete_confirm,
        toggle_proxy_confirm,
        warmup_confirm,
        selected_ids,
        message,
    };

    let filter_counts = Memo::new(move |_| {
        let accounts = state.accounts.get();
        let all = accounts.len();
        let pro = accounts
            .iter()
            .filter(|a| is_pro_tier(a.quota.as_ref().and_then(|q| q.subscription_tier.as_ref())))
            .count();
        let ultra = accounts
            .iter()
            .filter(|a| is_ultra_tier(a.quota.as_ref().and_then(|q| q.subscription_tier.as_ref())))
            .count();
        let free = all - pro - ultra;
        let needs_verification = accounts
            .iter()
            .filter(|a| needs_phone_verification(a.proxy_disabled_reason.as_ref()))
            .count();
        (all, pro, ultra, free, needs_verification)
    });

    let filtered_accounts = Memo::new(move |_| {
        let query = search_query.get().to_lowercase();
        let accounts = state.accounts.get();
        let current_filter = filter.get();

        accounts
            .into_iter()
            .filter(|a| {
                if !query.is_empty() && !a.email.to_lowercase().contains(&query) {
                    return false;
                }
                let tier = a.quota.as_ref().and_then(|q| q.subscription_tier.as_ref());
                match current_filter {
                    FilterType::All => true,
                    FilterType::Pro => is_pro_tier(tier),
                    FilterType::Ultra => is_ultra_tier(tier),
                    FilterType::Free => !is_pro_tier(tier) && !is_ultra_tier(tier),
                    FilterType::NeedsVerification => {
                        needs_phone_verification(a.proxy_disabled_reason.as_ref())
                    }
                }
            })
            .collect::<Vec<_>>()
    });

    let paginated_accounts = Memo::new(move |_| {
        let all = filtered_accounts.get();
        let page = current_page.get();
        let per_page = items_per_page.get();
        let start = (page - 1) * per_page;
        all.into_iter()
            .skip(start)
            .take(per_page)
            .collect::<Vec<_>>()
    });

    let total_pages = Memo::new(move |_| {
        let total = filtered_accounts.get().len();
        let per_page = items_per_page.get();
        (total + per_page - 1) / per_page.max(1)
    });

    let selected_count = Memo::new(move |_| selected_ids.get().len());
    let all_page_selected = Memo::new(move |_| {
        let page_accounts = paginated_accounts.get();
        let selected = selected_ids.get();
        !page_accounts.is_empty() && page_accounts.iter().all(|a| selected.contains(&a.id))
    });

    Effect::new(move |_| {
        let _ = filter.get();
        let _ = search_query.get();
        current_page.set(1);
        selected_ids.set(HashSet::new());
    });

    let on_switch_account = actions.create_switch_callback();
    let on_refresh_account = actions.create_refresh_callback();
    let on_warmup_account = actions.create_warmup_callback();

    let on_delete_account = Callback::new(move |account_id: String| {
        delete_confirm.set(Some(account_id));
    });

    let on_toggle_proxy = Callback::new(move |(account_id, current_disabled): (String, bool)| {
        toggle_proxy_confirm.set(Some((account_id, !current_disabled)));
    });

    let state_view = state.clone();
    let on_view_details = Callback::new(move |account_id: String| {
        let accounts = state_view.accounts.get();
        if let Some(account) = accounts.iter().find(|a: &&Account| a.id == account_id) {
            details_account.set(Some(account.clone()));
        }
    });

    let on_toggle_select = Callback::new(move |account_id: String| {
        selected_ids.update(|ids| {
            if ids.contains(&account_id) {
                ids.remove(&account_id);
            } else {
                ids.insert(account_id);
            }
        });
    });

    let on_toggle_all = move || {
        let page_ids: HashSet<String> = paginated_accounts
            .get()
            .iter()
            .map(|a| a.id.clone())
            .collect();
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

    let on_batch_delete = move || {
        if !selected_ids.get().is_empty() {
            batch_delete_confirm.set(true);
        }
    };

    let execute_delete = {
        let actions = actions.clone();
        move || actions.execute_delete()
    };

    let execute_batch_delete = {
        let actions = actions.clone();
        move || actions.execute_batch_delete()
    };

    let execute_toggle_proxy = {
        let actions = actions.clone();
        move || actions.execute_toggle_proxy()
    };

    let on_warmup_all = {
        let actions = actions.clone();
        move || actions.on_warmup_all()
    };

    let on_sync_local = {
        let actions = actions.clone();
        move || actions.on_sync_local()
    };

    let on_refresh_all_quotas = {
        let actions = actions.clone();
        move || actions.on_refresh_all_quotas()
    };

    view! {
        <div class="page accounts">
            <Header
                state=state.clone()
                sync_pending=sync_pending
                refresh_pending=refresh_pending
                warmup_pending=warmup_pending
                warmup_confirm=warmup_confirm
                add_account_modal_open=add_account_modal_open
                on_sync_local=on_sync_local
                on_refresh_all_quotas=on_refresh_all_quotas
            />

            <MessageBanner message=message />

            <Toolbar
                search_query=search_query
                view_mode=view_mode
                filter=filter
                filter_counts=filter_counts
                selected_count=selected_count
                on_batch_delete=on_batch_delete
            />

            <AccountsContent
                view_mode=view_mode
                paginated_accounts=paginated_accounts
                selected_ids=selected_ids
                refreshing_ids=refreshing_ids
                all_page_selected=all_page_selected
                on_toggle_all=on_toggle_all
                on_toggle_select=on_toggle_select
                on_switch_account=on_switch_account
                on_refresh_account=on_refresh_account
                on_warmup_account=on_warmup_account
                on_delete_account=on_delete_account
                on_toggle_proxy=on_toggle_proxy
                on_view_details=on_view_details
            />

            <Show when=move || { total_pages.get() > 1 }>
                <Pagination
                    current_page=Signal::derive(move || current_page.get())
                    total_pages=Signal::derive(move || total_pages.get())
                    total_items=Signal::derive(move || filtered_accounts.get().len())
                    items_per_page=Signal::derive(move || items_per_page.get())
                    on_page_change=on_page_change
                    on_page_size_change=on_page_size_change
                />
            </Show>

            <Modals
                delete_confirm=delete_confirm
                batch_delete_confirm=batch_delete_confirm
                toggle_proxy_confirm=toggle_proxy_confirm
                warmup_confirm=warmup_confirm
                add_account_modal_open=add_account_modal_open
                details_account=details_account
                selected_count=selected_count
                execute_delete=execute_delete
                execute_batch_delete=execute_batch_delete
                execute_toggle_proxy=execute_toggle_proxy
                on_warmup_all=on_warmup_all
                state=state.clone()
            />
        </div>
    }
}
