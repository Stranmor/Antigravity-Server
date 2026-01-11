//! Account card component for grid view

use crate::types::Account;
use leptos::prelude::*;

#[component]
pub fn AccountCard(
    #[prop(into)] account: Account,
    #[prop(into)] is_current: Signal<bool>,
    #[prop(into)] is_selected: Signal<bool>,
    #[prop(into)] is_refreshing: Signal<bool>,
    #[prop(into)] on_select: Callback<()>,
    #[prop(into)] on_switch: Callback<()>,
    #[prop(into)] on_refresh: Callback<()>,
    #[prop(into)] on_delete: Callback<()>,
    #[prop(into)] on_toggle_proxy: Callback<()>,
) -> impl IntoView {
    let email = account.email.clone();
    let email_short = email.split('@').next().unwrap_or(&email).to_string();
    let email_domain = email
        .split('@')
        .nth(1)
        .map(|s| s.to_string())
        .unwrap_or_default();
    let is_disabled = account.disabled;
    let proxy_disabled = account.proxy_disabled;

    // Compute quotas
    let gemini_quota = account
        .quota
        .as_ref()
        .map(|q| {
            q.models
                .iter()
                .find(|m| m.model.contains("gemini") || m.model.contains("flash"))
                .map(|m| {
                    if m.limit > 0 {
                        (m.limit - m.used) * 100 / m.limit
                    } else {
                        0
                    }
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);

    let claude_quota = account
        .quota
        .as_ref()
        .map(|q| {
            q.models
                .iter()
                .find(|m| m.model.contains("claude"))
                .map(|m| {
                    if m.limit > 0 {
                        (m.limit - m.used) * 100 / m.limit
                    } else {
                        0
                    }
                })
                .unwrap_or(0)
        })
        .unwrap_or(0);

    let tier = account
        .quota
        .as_ref()
        .and_then(|q| q.subscription_tier.clone())
        .unwrap_or_else(|| "Free".to_string());

    let tier_class = if tier.to_lowercase().contains("ultra") {
        "tier-ultra"
    } else if tier.to_lowercase().contains("pro") {
        "tier-pro"
    } else {
        "tier-free"
    };

    let gemini_class = quota_class(gemini_quota);
    let claude_class = quota_class(claude_quota);
    let tier_display = tier.clone();

    view! {
        <div
            class=move || format!(
                "account-card {} {} {}",
                if is_current.get() { "is-current" } else { "" },
                if is_selected.get() { "is-selected" } else { "" },
                if is_disabled { "is-disabled" } else { "" }
            )
            on:click=move |_| on_select.run(())
        >
            // Header
            <div class="account-card-header">
                <input
                    type="checkbox"
                    checked=move || is_selected.get()
                    on:click=|e| e.stop_propagation()
                    on:change=move |_| on_select.run(())
                />
                <span class=format!("tier-badge {}", tier_class)>{tier_display.clone()}</span>
                {move || is_current.get().then(|| view! {
                    <span class="current-badge">"ACTIVE"</span>
                })}
            </div>

            // Email
            <div class="account-card-email">
                <span class="email-name">{email_short.clone()}</span>
                <span class="email-domain">"@"{email_domain.clone()}</span>
            </div>

            // Quotas
            <div class="account-card-quotas">
                <div class="quota-item">
                    <span class="quota-label">"Gemini"</span>
                    <div class="quota-bar">
                        <div
                            class=format!("quota-fill {}", gemini_class)
                            style=format!("width: {}%", gemini_quota)
                        ></div>
                    </div>
                    <span class="quota-value">{gemini_quota}"%"</span>
                </div>
                <div class="quota-item">
                    <span class="quota-label">"Claude"</span>
                    <div class="quota-bar">
                        <div
                            class=format!("quota-fill {}", claude_class)
                            style=format!("width: {}%", claude_quota)
                        ></div>
                    </div>
                    <span class="quota-value">{claude_quota}"%"</span>
                </div>
            </div>

            // Proxy status
            <div class="account-card-proxy">
                <span class="proxy-label">"Proxy"</span>
                <button
                    class=format!("proxy-toggle {}", if proxy_disabled { "off" } else { "on" })
                    on:click=move |e| {
                        e.stop_propagation();
                        on_toggle_proxy.run(());
                    }
                >
                    {if proxy_disabled { "OFF" } else { "ON" }}
                </button>
            </div>

            // Actions
            <div class="account-card-actions">
                <button
                    class="btn btn--icon btn--sm"
                    title="Switch to this account"
                    on:click=move |e| {
                        e.stop_propagation();
                        on_switch.run(());
                    }
                >
                    "⚡"
                </button>
                <button
                    class=move || format!("btn btn--icon btn--sm {}", if is_refreshing.get() { "loading" } else { "" })
                    title="Refresh quota"
                    disabled=move || is_refreshing.get()
                    on:click=move |e| {
                        e.stop_propagation();
                        on_refresh.run(());
                    }
                >
                    "🔄"
                </button>
                <button
                    class="btn btn--icon btn--sm btn--danger"
                    title="Delete"
                    on:click=move |e| {
                        e.stop_propagation();
                        on_delete.run(());
                    }
                >
                    "🗑"
                </button>
            </div>
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
