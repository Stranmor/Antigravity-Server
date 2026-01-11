//! Dashboard page with full features

use crate::app::AppState;
use crate::components::{Button, ButtonVariant, StatsCard};
use crate::tauri::commands;
use crate::types::DashboardStats;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn Dashboard() -> impl IntoView {
    let state = expect_context::<AppState>();

    // Loading states
    let refresh_pending = RwSignal::new(false);
    let export_pending = RwSignal::new(false);
    let message = RwSignal::new(Option::<(String, bool)>::None);

    let show_message = move |msg: String, is_error: bool| {
        message.set(Some((msg, is_error)));
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            message.set(None);
        });
    };

    // Compute stats from accounts
    let stats = Memo::new(move |_| DashboardStats::from_accounts(&state.accounts.get()));

    // Current account info
    let current_account = Memo::new(move |_| {
        let current_id = state.current_account_id.get();
        current_id.and_then(|id| state.accounts.get().into_iter().find(|a| a.id == id))
    });

    // Best accounts by quota (top 5)
    let best_accounts = Memo::new(move |_| {
        let mut accounts = state.accounts.get();
        accounts.sort_by(|a, b| {
            let quota_a = a
                .quota
                .as_ref()
                .map(|q| {
                    q.models
                        .iter()
                        .map(|m| {
                            if m.limit > 0 {
                                (m.limit - m.used) * 100 / m.limit
                            } else {
                                0
                            }
                        })
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let quota_b = b
                .quota
                .as_ref()
                .map(|q| {
                    q.models
                        .iter()
                        .map(|m| {
                            if m.limit > 0 {
                                (m.limit - m.used) * 100 / m.limit
                            } else {
                                0
                            }
                        })
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            quota_b.cmp(&quota_a)
        });
        accounts.into_iter().take(5).collect::<Vec<_>>()
    });

    // Greeting
    let greeting = Memo::new(move |_| match current_account.get() {
        Some(account) => {
            let name = account.email.split('@').next().unwrap_or("User");
            format!("Hello, {}!", name)
        }
        None => "Welcome to Antigravity!".to_string(),
    });

    // Actions
    let on_refresh_current = move || {
        if let Some(account) = current_account.get_untracked() {
            refresh_pending.set(true);
            spawn_local(async move {
                match commands::fetch_account_quota(&account.id).await {
                    Ok(_) => {
                        if let Ok(accounts) = commands::list_accounts().await {
                            let state = expect_context::<AppState>();
                            state.accounts.set(accounts);
                        }
                        show_message("Quota refreshed".to_string(), false);
                    }
                    Err(e) => show_message(format!("Failed: {}", e), true),
                }
                refresh_pending.set(false);
            });
        }
    };

    let on_export_all = move || {
        export_pending.set(true);
        let accounts = state.accounts.get_untracked();
        spawn_local(async move {
            // Build export data
            let export_data: Vec<_> = accounts
                .iter()
                .filter_map(|acc| {
                    acc.tokens.as_ref().and_then(|t| t.refresh_token.clone()).map(|rt| {
                        serde_json::json!({
                            "email": acc.email,
                            "refresh_token": rt,
                        })
                    })
                })
                .collect();
            let content = serde_json::to_string_pretty(&export_data).unwrap_or_default();

            // Copy to clipboard for now (file dialog requires native integration)
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = clipboard.write_text(&content);
                show_message(format!("Exported {} accounts to clipboard", export_data.len()), false);
            } else {
                show_message("Export failed: clipboard unavailable".to_string(), true);
            }
            export_pending.set(false);
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

    view! {
        <div class="page dashboard">
            <header class="page-header">
                <div class="header-left">
                    <h1>{greeting}</h1>
                    <p class="subtitle">"Overview of your Antigravity accounts"</p>
                </div>
            <div class="header-actions">
                    <button
                        class="btn btn--secondary"
                        disabled=move || refresh_pending.get() || current_account.get().is_none()
                        on:click=move |_| on_refresh_current()
                    >
                        {move || if refresh_pending.get() { "Loading..." } else { "🔄 Refresh" }}
                    </button>
                    <Button
                        text="📥 Export".to_string()
                        variant=ButtonVariant::Secondary
                        on_click=on_export_all
                        loading=export_pending.get()
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

            // Stats grid
            <section class="stats-grid stats-grid--5">
                <StatsCard
                    title="Total Accounts".to_string()
                    value=Signal::derive(move || stats.get().total_accounts.to_string())
                    icon="👥".to_string()
                    color="blue".to_string()
                />
                <StatsCard
                    title="Avg Gemini Quota".to_string()
                    value=Signal::derive(move || format!("{}%", stats.get().avg_gemini_quota))
                    icon="✨".to_string()
                    color="green".to_string()
                    subtitle=Signal::derive(move || {
                        if stats.get().avg_gemini_quota >= 50 { "Sufficient" } else { "Low" }.to_string()
                    })
                />
                <StatsCard
                    title="Avg Image Quota".to_string()
                    value=Signal::derive(move || format!("{}%", stats.get().avg_gemini_image_quota))
                    icon="🎨".to_string()
                    color="purple".to_string()
                    subtitle=Signal::derive(move || {
                        if stats.get().avg_gemini_image_quota >= 50 { "Sufficient" } else { "Low" }.to_string()
                    })
                />
                <StatsCard
                    title="Avg Claude Quota".to_string()
                    value=Signal::derive(move || format!("{}%", stats.get().avg_claude_quota))
                    icon="🤖".to_string()
                    color="cyan".to_string()
                    subtitle=Signal::derive(move || {
                        if stats.get().avg_claude_quota >= 50 { "Sufficient" } else { "Low" }.to_string()
                    })
                />
                <StatsCard
                    title="Low Quota".to_string()
                    value=Signal::derive(move || stats.get().low_quota_count.to_string())
                    icon="⚠️".to_string()
                    color="orange".to_string()
                    subtitle=Signal::derive(|| "< 20% remaining".to_string())
                />
            </section>

            // Two column layout
            <div class="dashboard-columns">
                // Current Account
                <section class="dashboard-card">
                    <h2>"Current Account"</h2>
                    {move || match current_account.get() {
                        Some(account) => {
                            let gemini_quota = account.quota.as_ref().map(|q| {
                                q.models.iter()
                                    .find(|m| m.model.contains("gemini") || m.model.contains("flash"))
                                    .map(|m| if m.limit > 0 { (m.limit - m.used) * 100 / m.limit } else { 0 })
                                    .unwrap_or(0)
                            }).unwrap_or(0);

                            let claude_quota = account.quota.as_ref().map(|q| {
                                q.models.iter()
                                    .find(|m| m.model.contains("claude"))
                                    .map(|m| if m.limit > 0 { (m.limit - m.used) * 100 / m.limit } else { 0 })
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

                // Best Accounts
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

                                let max_quota = account.quota.as_ref().map(|q| {
                                    q.models.iter()
                                        .map(|m| if m.limit > 0 { (m.limit - m.used) * 100 / m.limit } else { 0 })
                                        .max()
                                        .unwrap_or(0)
                                }).unwrap_or(0);

                                let tier = account.quota.as_ref()
                                    .and_then(|q| q.subscription_tier.clone())
                                    .unwrap_or_else(|| "Free".to_string());
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
                                            <button
                                                class="btn btn--icon btn--sm"
                                                title="Switch"
                                                on:click={
                                                    let id = account.id.clone();
                                                    move |_| on_switch_account(id.clone())
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
            </div>

            // Tier breakdown
            <section class="tier-section">
                <h2>"Account Tiers"</h2>
                <div class="tier-grid">
                    <div class="tier-card tier-card--ultra">
                        <span class="tier-count">{move || stats.get().ultra_count}</span>
                        <span class="tier-label">"Ultra"</span>
                    </div>
                    <div class="tier-card tier-card--pro">
                        <span class="tier-count">{move || stats.get().pro_count}</span>
                        <span class="tier-label">"Pro"</span>
                    </div>
                    <div class="tier-card tier-card--free">
                        <span class="tier-count">{move || stats.get().free_count}</span>
                        <span class="tier-label">"Free"</span>
                    </div>
                    <div class="tier-card tier-card--warning">
                        <span class="tier-count">{move || stats.get().low_quota_count}</span>
                        <span class="tier-label">"Low Quota"</span>
                    </div>
                </div>
            </section>

            // Quick actions
            <section class="quick-actions">
                <h2>"Quick Actions"</h2>
                <div class="action-grid">
                    <a href="/accounts" class="action-card">
                        <span class="action-icon">"➕"</span>
                        <span class="action-label">"Add Account"</span>
                    </a>
                    <a href="/proxy" class="action-card">
                        <span class="action-icon">"🔌"</span>
                        <span class="action-label">"Start Proxy"</span>
                    </a>
                    <a href="/monitor" class="action-card">
                        <span class="action-icon">"📡"</span>
                        <span class="action-label">"View Logs"</span>
                    </a>
                    <a href="/settings" class="action-card">
                        <span class="action-icon">"⚙️"</span>
                        <span class="action-label">"Settings"</span>
                    </a>
                </div>
            </section>
        </div>
    }
}
