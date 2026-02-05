//! Dashboard sections: current account, best accounts, tiers, quick actions

use crate::api_models::Account;
use leptos::prelude::*;

/// Current account section showing active account details.
#[component]
pub(crate) fn CurrentAccountSection(current_account: Memo<Option<Account>>) -> impl IntoView {
    view! {
        <section class="dashboard-card">
            <h2>"Current Account"</h2>
            {move || match current_account.get() {
                Some(account) => {
                    let gemini_quota = account.quota.as_ref().map(|q| {
                        q.models.iter()
                            .find(|m| m.name.contains("gemini") || m.name.contains("flash"))
                            .map(|m| m.percentage)
                            .unwrap_or(0)
                    }).unwrap_or(0);

                    let claude_quota = account.quota.as_ref().map(|q| {
                        q.models.iter()
                            .find(|m| m.name.contains("claude"))
                            .map(|m| m.percentage)
                            .unwrap_or(0)
                    }).unwrap_or(0);

                    let tier = account.quota.as_ref()
                        .and_then(|q| q.subscription_tier.clone())
                        .unwrap_or_else(|| "Free".to_string());
                    let tier_class = tier.to_lowercase();
                    let tier_display = tier.clone();

                    view! {
                        <div class="current-account-detail">
                            <div class="account-header">
                                <span class="account-email">{account.email.clone()}</span>
                                <span class=format!("tier-badge tier-{}", tier_class)>{tier_display}</span>
                            </div>
                            <div class="quota-bars">
                                <div class="quota-row">
                                    <span>"Gemini"</span>
                                    <div class="quota-bar">
                                        <div class="quota-fill" style=format!("width: {}%", gemini_quota)></div>
                                    </div>
                                    <span>{gemini_quota}"%"</span>
                                </div>
                                <div class="quota-row">
                                    <span>"Claude"</span>
                                    <div class="quota-bar">
                                        <div class="quota-fill quota-fill--claude" style=format!("width: {}%", claude_quota)></div>
                                    </div>
                                    <span>{claude_quota}"%"</span>
                                </div>
                            </div>
                            <a href="/accounts" class="btn btn--secondary btn--block">"Switch Account"</a>
                        </div>
                    }.into_any()
                }
                None => view! {
                    <div class="no-account">
                        <span class="empty-icon">"👤"</span>
                        <p>"No account selected"</p>
                        <a href="/accounts" class="btn btn--primary">"Select Account"</a>
                    </div>
                }.into_any()
            }}
        </section>
    }
}
