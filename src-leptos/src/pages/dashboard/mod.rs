//! Dashboard page with full features

pub(crate) mod best_accounts;
pub(crate) mod current_account;
pub(crate) mod tiers;

use best_accounts::BestAccountsSection;
use current_account::CurrentAccountSection;
use tiers::{QuickActionsSection, TierSection};

use crate::api::commands;
use crate::api_models::DashboardStats;
use crate::app::AppState;
use crate::components::{Button, ButtonVariant, StatsCard};
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Dashboard page with account overview and statistics.
#[component]
pub(crate) fn Dashboard() -> impl IntoView {
    let state = expect_context::<AppState>();

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

    let stats = Memo::new(move |_| DashboardStats::from_accounts(&state.accounts.get()));

    let current_account = Memo::new(move |_| {
        let current_id = state.current_account_id.get();
        current_id.and_then(|id| state.accounts.get().into_iter().find(|a| a.id == id))
    });

    let best_accounts = Memo::new(move |_| {
        let mut accounts = state.accounts.get();
        accounts.sort_by(|a, b| {
            let quota_a = a
                .quota
                .as_ref()
                .map(|q| q.models.iter().map(|m| m.percentage).max().unwrap_or(0))
                .unwrap_or(0);
            let quota_b = b
                .quota
                .as_ref()
                .map(|q| q.models.iter().map(|m| m.percentage).max().unwrap_or(0))
                .unwrap_or(0);
            quota_b.cmp(&quota_a)
        });
        accounts.into_iter().take(5).collect::<Vec<_>>()
    });

    let greeting = Memo::new(move |_| match current_account.get() {
        Some(account) => {
            let name = account.email.split('@').next().unwrap_or("User");
            format!("Hello, {}!", name)
        },
        None => "Welcome to Antigravity!".to_string(),
    });

    let state_refresh = state.clone();
    let state_export = state.clone();
    let state_switch = state.clone();

    let on_refresh_current = move || {
        if let Some(account) = current_account.get_untracked() {
            refresh_pending.set(true);
            let s = state_refresh.clone();
            spawn_local(async move {
                match commands::fetch_account_quota(&account.id).await {
                    Ok(_) => {
                        if let Ok(accounts) = commands::list_accounts().await {
                            s.accounts.set(accounts);
                        }
                        show_message("Quota refreshed".to_string(), false);
                    },
                    Err(e) => show_message(format!("Failed: {}", e), true),
                }
                refresh_pending.set(false);
            });
        }
    };

    let on_export_all = move || {
        export_pending.set(true);
        let accounts = state_export.accounts.get_untracked();
        spawn_local(async move {
            let export_data: Vec<_> = accounts
                .iter()
                .filter_map(|acc| {
                    let rt = acc.token.refresh_token.clone();
                    if rt.is_empty() {
                        None
                    } else {
                        Some(serde_json::json!({"email": acc.email, "refresh_token": rt}))
                    }
                })
                .collect();
            let content = serde_json::to_string_pretty(&export_data).unwrap_or_default();

            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = clipboard.write_text(&content);
                show_message(
                    format!("Exported {} accounts to clipboard", export_data.len()),
                    false,
                );
            } else {
                show_message("Export failed: clipboard unavailable".to_string(), true);
            }
            export_pending.set(false);
        });
    };

    let on_switch_account = Callback::new(move |account_id: String| {
        let s = state_switch.clone();
        spawn_local(async move {
            if commands::switch_account(&account_id).await.is_ok() {
                s.current_account_id.set(Some(account_id));
            }
        });
    });

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

            <Show when=move || message.get().is_some()>
                {move || {
                    let Some((msg, is_error)) = message.get() else {
                        return view! { <div></div> }.into_any();
                    };
                    view! {
                        <div class=format!("alert {}", if is_error { "alert--error" } else { "alert--success" })>
                            <span>{msg}</span>
                        </div>
                    }.into_any()
                }}
            </Show>

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

            <div class="dashboard-columns">
                <CurrentAccountSection current_account=current_account />
                <BestAccountsSection best_accounts=best_accounts on_switch_account=on_switch_account />
            </div>

            <TierSection stats=stats />
            <QuickActionsSection />
        </div>
    }
}
