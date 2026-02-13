//! Account table row component for list view

use crate::api_models::Account;
use crate::app::AppState;
use crate::formatters::{format_time_remaining, get_time_remaining_color};
use leptos::prelude::*;
use std::collections::HashSet;

use super::filter_types::{format_tier_display, quota_class};

/// Account table row component for list view.
#[component]
pub(crate) fn AccountRow(
    account: Account,
    selected_ids: RwSignal<HashSet<String>>,
    refreshing_ids: RwSignal<HashSet<String>>,
    on_toggle_select: Callback<String>,
    on_switch_account: Callback<String>,
    on_refresh_account: Callback<String>,
    on_warmup_account: Callback<String>,
    on_delete_account: Callback<String>,
    on_toggle_proxy: Callback<(String, bool)>,
    on_view_details: Callback<String>,
) -> impl IntoView {
    let state = expect_context::<AppState>();

    let account_id = account.id.clone();
    let account_id2 = account.id.clone();
    let account_id3 = account.id.clone();
    let account_id4 = account.id.clone();
    let account_id5 = account.id.clone();
    let account_id6 = account.id.clone();
    let account_id7 = account.id.clone();
    let account_id_select = account.id.clone();
    let account_id_switch = account.id.clone();
    let account_id_delete = account.id.clone();
    let account_id_proxy = account.id.clone();
    let email = account.email.clone();
    let is_disabled = account.disabled;
    let proxy_disabled = account.proxy_disabled;
    let disabled_reason = account.proxy_disabled_reason.clone();
    let needs_verification =
        disabled_reason.as_ref().is_some_and(|r| r == "phone_verification_required");
    let is_tos_banned = disabled_reason.as_ref().is_some_and(|r| {
        r.contains("tos_ban") || r.contains("banned") || r.contains("USER_DISABLED")
    });
    let is_locked =
        !is_tos_banned && !needs_verification && disabled_reason.is_some() && proxy_disabled;

    let tier_raw = account
        .quota
        .as_ref()
        .and_then(|q| q.subscription_tier.clone())
        .unwrap_or_else(|| "Free".to_string());

    let tier = format_tier_display(&tier_raw);

    let tier_class = if tier.to_lowercase().contains("ultra") {
        "tier-ultra"
    } else if tier.to_lowercase().contains("pro") {
        "tier-pro"
    } else {
        "tier-free"
    };

    let g3_pro = account
        .quota
        .as_ref()
        .and_then(|q| q.models.iter().find(|m| m.name.to_lowercase() == "gemini-3-pro-high"));
    let g3_flash = account
        .quota
        .as_ref()
        .and_then(|q| q.models.iter().find(|m| m.name.to_lowercase() == "gemini-3-flash"));
    let g3_image = account
        .quota
        .as_ref()
        .and_then(|q| q.models.iter().find(|m| m.name.to_lowercase() == "gemini-3-pro-image"));
    let claude = account
        .quota
        .as_ref()
        .and_then(|q| q.models.iter().find(|m| m.name.to_lowercase() == "claude-sonnet-4-5"));

    let quota_g3_pro = g3_pro.map(|m| m.percentage).unwrap_or(0);
    let reset_g3_pro = g3_pro.map(|m| m.reset_time.clone()).unwrap_or_default();
    let quota_g3_flash = g3_flash.map(|m| m.percentage).unwrap_or(0);
    let reset_g3_flash = g3_flash.map(|m| m.reset_time.clone()).unwrap_or_default();
    let quota_g3_image = g3_image.map(|m| m.percentage).unwrap_or(0);
    let reset_g3_image = g3_image.map(|m| m.reset_time.clone()).unwrap_or_default();
    let quota_claude = claude.map(|m| m.percentage).unwrap_or(0);
    let reset_claude = claude.map(|m| m.reset_time.clone()).unwrap_or_default();
    let all_quota_zero =
        quota_g3_pro == 0 && quota_g3_flash == 0 && quota_g3_image == 0 && quota_claude == 0;
    // Banned = all quotas 0% AND no reset times (won't recover)
    // Exhausted = all quotas 0% but has reset times (will recover)
    let has_any_reset = !reset_g3_pro.is_empty()
        || !reset_g3_flash.is_empty()
        || !reset_g3_image.is_empty()
        || !reset_claude.is_empty();
    let is_quota_banned =
        all_quota_zero && !has_any_reset && !is_disabled && account.quota.is_some();
    let show_banned = is_tos_banned || is_quota_banned;

    let account_id_class = account_id.clone();
    let account_id_class2 = account_id2.clone();
    let account_id_status = account_id4.clone();
    let account_id_show = account_id.clone();
    let account_id_refresh = account_id3.clone();

    view! {
        <tr class=move || format!(
            "account-row {} {}",
            if state.current_account_id.get() == Some(account_id_class.clone()) { "is-current" } else { "" },
            if selected_ids.get().contains(&account_id_class2) { "is-selected" } else { "" }
        )>
            <td class="col-checkbox">
                <input
                    type="checkbox"
                    checked=move || selected_ids.get().contains(&account_id2.clone())
                    on:change={
                        let id = account_id_select.clone();
                        move |_| on_toggle_select.run(id.clone())
                    }
                />
            </td>
            <td class="col-status">
                <span class=move || {
                    let cls = if show_banned {
                        "status-dot--banned"
                    } else if state.current_account_id.get() == Some(account_id_status.clone()) {
                        "status-dot--active"
                    } else if is_disabled {
                        "status-dot--disabled"
                    } else if is_locked {
                        "status-dot--locked"
                    } else {
                        "status-dot--idle"
                    };
                    format!("status-dot {}", cls)
                }></span>
            </td>
            <td class="col-email">
                <span class="email-text">{email.clone()}</span>
                <Show when=move || state.current_account_id.get() == Some(account_id_show.clone())>
                    <span class="current-badge">"ACTIVE"</span>
                </Show>
                {show_banned.then(|| view! {
                    <span class="banned-badge" title="Account banned (TOS violation or all quotas exhausted)">"🚫 BANNED"</span>
                })}
                {is_locked.then(|| view! {
                    <span class="locked-badge" title=format!("Locked: {}", disabled_reason.clone().unwrap_or_default())>"⚠️ LOCKED"</span>
                })}
                {needs_verification.then(|| view! {
                    <span class="verify-badge" title="Phone verification required">"📱"</span>
                })}
            </td>
            <td class="col-tier">
                <span class=format!("tier-badge {}", tier_class)>{tier}</span>
            </td>
            <td class="col-model-quota">
                <div class="model-quota-grid">
                    <QuotaCell label="G3 Pro" percent=quota_g3_pro reset_time=reset_g3_pro />
                    <QuotaCell label="G3 Flash" percent=quota_g3_flash reset_time=reset_g3_flash />
                    <QuotaCell label="G3 Image" percent=quota_g3_image reset_time=reset_g3_image />
                    <QuotaCell label="Claude" percent=quota_claude reset_time=reset_claude />
                </div>
            </td>
            <td class="col-proxy">
                <button
                    class=format!("proxy-badge {}", if proxy_disabled { "off" } else { "on" })
                    on:click={
                        let id = account_id_proxy.clone();
                        move |_| on_toggle_proxy.run((id.clone(), proxy_disabled))
                    }
                >
                    {if proxy_disabled { "OFF" } else { "ON" }}
                </button>
            </td>
            <td class="col-actions">
                <button
                    class="btn btn--icon"
                    title="View Details"
                    on:click={
                        let id = account_id.clone();
                        move |_| on_view_details.run(id.clone())
                    }
                >"📊"</button>
                <button
                    class="btn btn--icon"
                    title="Switch"
                    on:click={
                        let id = account_id_switch.clone();
                        move |_| on_switch_account.run(id.clone())
                    }
                >"⚡"</button>
                <button
                    class={
                        let id = account_id_refresh.clone();
                        move || format!("btn btn--icon {}", if refreshing_ids.get().contains(&id.clone()) { "loading" } else { "" })
                    }
                    title="Refresh"
                    disabled=move || refreshing_ids.get().contains(&account_id3.clone())
                    on:click={
                        let id = account_id_refresh.clone();
                        move |_| on_refresh_account.run(id.clone())
                    }
                >"🔄"</button>
                <button
                    class={
                        let id = account_id5.clone();
                        move || format!("btn btn--icon {}", if refreshing_ids.get().contains(&id.clone()) { "loading" } else { "" })
                    }
                    title="Warmup"
                    disabled=move || refreshing_ids.get().contains(&account_id6.clone())
                    on:click={
                        let id = account_id7.clone();
                        move |_| on_warmup_account.run(id.clone())
                    }
                >"✨"</button>
                <button
                    class="btn btn--icon btn--danger"
                    title="Delete"
                    on:click={
                        let id = account_id_delete.clone();
                        move |_| on_delete_account.run(id.clone())
                    }
                >"🗑"</button>
            </td>
        </tr>
    }
}

/// Quota display cell with progress bar and reset timer.
#[component]
fn QuotaCell(label: &'static str, percent: i32, reset_time: String) -> impl IntoView {
    let has_reset = !reset_time.is_empty();
    let color_class = if has_reset {
        format!("reset-time--{}", get_time_remaining_color(&reset_time))
    } else {
        String::new()
    };
    let formatted = if has_reset { format_time_remaining(&reset_time) } else { String::new() };

    view! {
        <div class="quota-cell">
            <span class="quota-label">{label}</span>
            <div class="quota-bar">
                <div
                    class=format!("quota-fill {}", quota_class(percent))
                    style=format!("width: {}%", percent)
                ></div>
            </div>
            {has_reset.then(|| view! {
                <span class=format!("quota-reset {}", color_class)>
                    "⏱ "{formatted}
                </span>
            })}
            <span class="quota-text">{percent}"%"</span>
        </div>
    }
}
