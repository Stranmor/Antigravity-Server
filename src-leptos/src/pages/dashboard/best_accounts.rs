//! Best accounts list component

use crate::api_models::Account;
use crate::app::AppState;
use crate::formatters::{format_time_remaining, get_time_remaining_color};
use crate::pages::accounts::filter_types::format_tier_display;
use leptos::prelude::*;

/// Best accounts list showing top quota accounts.
#[component]
pub(crate) fn BestAccountsSection(
    best_accounts: Memo<Vec<Account>>,
    on_switch_account: Callback<String>,
) -> impl IntoView {
    let state = expect_context::<AppState>();

    view! {
        <section class="dashboard-card">
            <h2>"Top Accounts"</h2>
            <div class="best-accounts-list">
                <For
                    each=move || best_accounts.get()
                    key=|a| a.id.clone()
                    children=move |account| {
                        let account_id = account.id.clone();
                        let email = account.email.clone();
                        let email_short = email.split('@').next().unwrap_or(&email).to_string();
                        let is_current = Memo::new(move |_| {
                            state.current_account_id.get() == Some(account_id.clone())
                        });

                        let best_model = account.quota.as_ref().and_then(|q| {
                            q.models.iter().max_by_key(|m| m.percentage)
                        });
                        let max_quota = best_model.map(|m| m.percentage).unwrap_or(0);
                        let reset_time = best_model.map(|m| m.reset_time.clone()).unwrap_or_default();

                        let tier_raw = account.quota.as_ref()
                            .and_then(|q| q.subscription_tier.clone())
                            .unwrap_or_else(|| "Free".to_string());
                        let tier = format_tier_display(&tier_raw);
                        let tier_class = tier.to_lowercase();
                        let tier_display = tier.clone();

                        view! {
                            <div class=move || format!("best-account-item {}", if is_current.get() { "is-current" } else { "" })>
                                <div class="account-info">
                                    <span class="email">{email_short}</span>
                                    <span class=format!("tier-badge tier-badge--sm tier-{}", tier_class)>{tier_display}</span>
                                    <Show when=move || is_current.get()>
                                        <span class="current-badge">"ACTIVE"</span>
                                    </Show>
                                </div>
                                <div class="quota-info">
                                    <span class="quota-value">{max_quota}"%"</span>
                                    {if !reset_time.is_empty() {
                                        let color_class = format!("reset-time--{}", get_time_remaining_color(&reset_time));
                                        let formatted = format_time_remaining(&reset_time);
                                        Some(view! {
                                            <span class=format!("quota-reset {}", color_class)>
                                                "⏱ "{formatted}
                                            </span>
                                        })
                                    } else {
                                        None
                                    }}
                                    <button
                                        class="btn btn--icon btn--sm"
                                        title="Switch"
                                        on:click={
                                            let cb = on_switch_account;
                                            let id = account.id.clone();
                                            move |_| cb.run(id.clone())
                                        }
                                    >"⚡"</button>
                                </div>
                            </div>
                        }
                    }
                />
                <Show when=move || best_accounts.get().is_empty()>
                    <p class="empty-text">"No accounts"</p>
                </Show>
            </div>
        </section>
    }
}
