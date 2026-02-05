use crate::api_models::Account;
use crate::app::AppState;
use crate::components::AccountCard;
use leptos::prelude::*;
use std::collections::HashSet;

use super::account_row::AccountRow;
use super::filter_types::ViewMode;

/// Accounts content area with list/grid view toggle.
#[component]
pub(crate) fn AccountsContent(
    view_mode: RwSignal<ViewMode>,
    paginated_accounts: Memo<Vec<Account>>,
    selected_ids: RwSignal<HashSet<String>>,
    refreshing_ids: RwSignal<HashSet<String>>,
    all_page_selected: Memo<bool>,
    on_toggle_all: impl Fn() + Send + Sync + 'static + Clone,
    on_toggle_select: Callback<String>,
    on_switch_account: Callback<String>,
    on_refresh_account: Callback<String>,
    on_warmup_account: Callback<String>,
    on_delete_account: Callback<String>,
    on_toggle_proxy: Callback<(String, bool)>,
    on_view_details: Callback<String>,
) -> impl IntoView {
    let state = expect_context::<AppState>();

    view! {
        <Show
            when=move || matches!(view_mode.get(), ViewMode::List)
            fallback=move || {
                let on_toggle_select = on_toggle_select;
                let on_switch_account = on_switch_account;
                let on_refresh_account = on_refresh_account;
                let on_delete_account = on_delete_account;
                let on_toggle_proxy = on_toggle_proxy;
                view! {
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
                                        on_select=Callback::new(move |_| on_toggle_select.run(id4.clone()))
                                        on_switch=Callback::new(move |_| on_switch_account.run(id5.clone()))
                                        on_refresh=Callback::new(move |_| on_refresh_account.run(id6.clone()))
                                        on_delete=Callback::new({
                                            let id = account.id.clone();
                                            move |_| on_delete_account.run(id.clone())
                                        })
                                        on_toggle_proxy=Callback::new({
                                            let id = account.id.clone();
                                            move |_| on_toggle_proxy.run((id.clone(), proxy_disabled))
                                        })
                                    />
                                }
                            }
                        />
                    </div>
                }
            }
        >
            {
                let on_toggle_all = on_toggle_all.clone();
                let on_toggle_select = on_toggle_select;
                let on_switch_account = on_switch_account;
                let on_refresh_account = on_refresh_account;
                let on_warmup_account = on_warmup_account;
                let on_delete_account = on_delete_account;
                let on_toggle_proxy = on_toggle_proxy;
                let on_view_details = on_view_details;
                view! {
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
                                    <th class="col-model-quota">"MODEL QUOTA"</th>
                                    <th class="col-proxy">"Proxy"</th>
                                    <th class="col-actions">"Actions"</th>
                                </tr>
                            </thead>
                            <tbody>
                                <For
                                    each=move || paginated_accounts.get()
                                    key=|account| account.id.clone()
                                    children=move |account| {
                                        view! {
                                            <AccountRow
                                                account=account
                                                selected_ids=selected_ids
                                                refreshing_ids=refreshing_ids
                                                on_toggle_select=on_toggle_select
                                                on_switch_account=on_switch_account
                                                on_refresh_account=on_refresh_account
                                                on_warmup_account=on_warmup_account
                                                on_delete_account=on_delete_account
                                                on_toggle_proxy=on_toggle_proxy
                                                on_view_details=on_view_details
                                            />
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
                }
            }
        </Show>
    }
}
