//! Account card component for grid view

use crate::api_models::Account;
use crate::formatters::{format_time_remaining, get_time_remaining_color};
use crate::pages::accounts::filter_types::quota_class;
use leptos::prelude::*;

#[component]
pub(crate) fn AccountCard(
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
    let email_domain = email.split('@').nth(1).map(|s| s.to_string()).unwrap_or_default();
    let is_disabled = account.disabled;
    let proxy_disabled = account.proxy_disabled;

    // Find 4 specific models by exact name (matching upstream)
    let g3_pro = account.quota.as_ref().and_then(|q| {
        q.models.iter().find(|m| m.name.to_lowercase() == "gemini-3-pro-high").cloned()
    });
    let g3_flash = account
        .quota
        .as_ref()
        .and_then(|q| q.models.iter().find(|m| m.name.to_lowercase() == "gemini-3-flash").cloned());
    let g3_image = account.quota.as_ref().and_then(|q| {
        q.models.iter().find(|m| m.name.to_lowercase() == "gemini-3-pro-image").cloned()
    });
    let claude = account.quota.as_ref().and_then(|q| {
        q.models.iter().find(|m| m.name.to_lowercase() == "claude-sonnet-4-5").cloned()
    });

    let quota_g3_pro = g3_pro.as_ref().map(|m| m.percentage).unwrap_or(0);
    let reset_g3_pro = g3_pro.as_ref().map(|m| m.reset_time.clone()).unwrap_or_default();
    let quota_g3_flash = g3_flash.as_ref().map(|m| m.percentage).unwrap_or(0);
    let reset_g3_flash = g3_flash.as_ref().map(|m| m.reset_time.clone()).unwrap_or_default();
    let quota_g3_image = g3_image.as_ref().map(|m| m.percentage).unwrap_or(0);
    let reset_g3_image = g3_image.as_ref().map(|m| m.reset_time.clone()).unwrap_or_default();
    let quota_claude = claude.as_ref().map(|m| m.percentage).unwrap_or(0);
    let reset_claude = claude.as_ref().map(|m| m.reset_time.clone()).unwrap_or_default();

    let g3_pro_reset_formatted = format_time_remaining(&reset_g3_pro);
    let g3_flash_reset_formatted = format_time_remaining(&reset_g3_flash);
    let g3_image_reset_formatted = format_time_remaining(&reset_g3_image);
    let claude_reset_formatted = format_time_remaining(&reset_claude);

    let g3_pro_reset_color = reset_time_class(get_time_remaining_color(&reset_g3_pro));
    let g3_flash_reset_color = reset_time_class(get_time_remaining_color(&reset_g3_flash));
    let g3_image_reset_color = reset_time_class(get_time_remaining_color(&reset_g3_image));
    let claude_reset_color = reset_time_class(get_time_remaining_color(&reset_claude));

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

    let g3_pro_class = quota_class(quota_g3_pro);
    let g3_flash_class = quota_class(quota_g3_flash);
    let g3_image_class = quota_class(quota_g3_image);
    let claude_class = quota_class(quota_claude);
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

            // Quotas - 2x2 grid matching upstream
            <div class="account-card-quotas model-quota-grid">
                // G3 Pro
                <div class="quota-item">
                    <div class="quota-header">
                        <span class="quota-label">"G3 Pro"</span>
                        <span class=format!("quota-reset {}", g3_pro_reset_color)>
                            "⏱ "{g3_pro_reset_formatted.clone()}
                        </span>
                    </div>
                    <div class="quota-bar">
                        <div
                            class=format!("quota-fill {}", g3_pro_class)
                            style=format!("width: {}%", quota_g3_pro)
                        ></div>
                    </div>
                    <span class="quota-value">{quota_g3_pro}"%"</span>
                </div>
                // G3 Flash
                <div class="quota-item">
                    <div class="quota-header">
                        <span class="quota-label">"G3 Flash"</span>
                        <span class=format!("quota-reset {}", g3_flash_reset_color)>
                            "⏱ "{g3_flash_reset_formatted.clone()}
                        </span>
                    </div>
                    <div class="quota-bar">
                        <div
                            class=format!("quota-fill {}", g3_flash_class)
                            style=format!("width: {}%", quota_g3_flash)
                        ></div>
                    </div>
                    <span class="quota-value">{quota_g3_flash}"%"</span>
                </div>
                // G3 Image
                <div class="quota-item">
                    <div class="quota-header">
                        <span class="quota-label">"G3 Image"</span>
                        <span class=format!("quota-reset {}", g3_image_reset_color)>
                            "⏱ "{g3_image_reset_formatted.clone()}
                        </span>
                    </div>
                    <div class="quota-bar">
                        <div
                            class=format!("quota-fill {}", g3_image_class)
                            style=format!("width: {}%", quota_g3_image)
                        ></div>
                    </div>
                    <span class="quota-value">{quota_g3_image}"%"</span>
                </div>
                // Claude
                <div class="quota-item">
                    <div class="quota-header">
                        <span class="quota-label">"Claude"</span>
                        <span class=format!("quota-reset {}", claude_reset_color)>
                            "⏱ "{claude_reset_formatted.clone()}
                        </span>
                    </div>
                    <div class="quota-bar">
                        <div
                            class=format!("quota-fill {}", claude_class)
                            style=format!("width: {}%", quota_claude)
                        ></div>
                    </div>
                    <span class="quota-value">{quota_claude}"%"</span>
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

fn reset_time_class(color: &str) -> &'static str {
    match color {
        "success" => "reset-time--success",
        "warning" => "reset-time--warning",
        _ => "reset-time--neutral",
    }
}
