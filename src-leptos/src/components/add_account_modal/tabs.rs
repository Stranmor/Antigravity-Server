//! Tab content components for add account modal

use super::parser::parse_refresh_tokens;
use super::AddAccountStatus;
use crate::api::commands;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn OAuthTab(
    status: RwSignal<AddAccountStatus>,
    message: RwSignal<String>,
    oauth_url: RwSignal<String>,
    url_copied: RwSignal<bool>,
    is_open: RwSignal<bool>,
    on_account_added: Callback<()>,
) -> impl IntoView {
    let on_copy_url = move |_| {
        let url = oauth_url.get();
        if !url.is_empty() {
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = clipboard.write_text(&url);
                url_copied.set(true);
                spawn_local(async move {
                    gloo_timers::future::TimeoutFuture::new(1500).await;
                    url_copied.set(false);
                });
            }
        }
    };

    let on_open_url = move |_| {
        let url = oauth_url.get();
        if !url.is_empty() {
            if let Some(window) = web_sys::window() {
                let _ = window.open_with_url_and_target(&url, "_blank");
            }
        }
    };

    let on_start_oauth = move |_| {
        status.set(AddAccountStatus::Loading);
        message.set("Opening browser for OAuth...".to_string());
        let url = oauth_url.get();
        if !url.is_empty() {
            if let Some(window) = web_sys::window() {
                let _ = window.open_with_url_and_target(&url, "_blank");
            }
        }
        message
            .set("Complete authorization in your browser, then click 'Finish OAuth'.".to_string());
    };

    let on_finish_oauth = move |_| {
        status.set(AddAccountStatus::Loading);
        message.set("Checking for new account...".to_string());
        spawn_local(async move {
            match commands::list_accounts().await {
                Ok(_) => {
                    status.set(AddAccountStatus::Success);
                    message.set("Account added! Refresh the list to see it.".to_string());
                    on_account_added.run(());
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(1500).await;
                        is_open.set(false);
                    });
                }
                Err(e) => {
                    status.set(AddAccountStatus::Error);
                    message.set(format!("Error: {}", e));
                }
            }
        });
    };

    view! {
        <div class="oauth-tab">
            <div class="oauth-icon">"🌐"</div>
            <h3>"Google OAuth (Recommended)"</h3>
            <p class="oauth-desc">"Authorize with your Google account to add it to Antigravity."</p>

            <button
                class="btn btn--primary btn--full"
                on:click=on_start_oauth
                disabled=move || matches!(status.get(), AddAccountStatus::Loading | AddAccountStatus::Success)
            >
                {move || if matches!(status.get(), AddAccountStatus::Loading) {
                    "Waiting for authorization..."
                } else {
                    "Start OAuth"
                }}
            </button>

            <Show when=move || !oauth_url.get().is_empty()>
                <div class="oauth-url-section">
                    <label class="oauth-url-label">"Or copy the URL manually:"</label>
                    <div class="oauth-url-box">
                        <code class="oauth-url-text">{move || {
                            let url = oauth_url.get();
                            if url.len() > 60 { format!("{}...", &url[..60]) } else { url }
                        }}</code>
                        <button class="btn btn--small btn--secondary" on:click=on_copy_url title="Copy URL">
                            {move || if url_copied.get() { "✓ Copied" } else { "📋 Copy" }}
                        </button>
                        <button class="btn btn--small btn--secondary" on:click=on_open_url title="Open in browser">
                            "🔗 Open"
                        </button>
                    </div>
                    <button
                        class="btn btn--secondary btn--full"
                        on:click=on_finish_oauth
                        disabled=move || matches!(status.get(), AddAccountStatus::Loading | AddAccountStatus::Success)
                    >
                        "✓ I've completed authorization - Finish"
                    </button>
                </div>
            </Show>
        </div>
    }
}

#[component]
pub fn TokenTab(
    status: RwSignal<AddAccountStatus>,
    message: RwSignal<String>,
    refresh_token_input: RwSignal<String>,
    is_open: RwSignal<bool>,
    on_account_added: Callback<()>,
) -> impl IntoView {
    let on_submit_token = move |_| {
        let input = refresh_token_input.get();
        if input.trim().is_empty() {
            status.set(AddAccountStatus::Error);
            message.set("Please enter a refresh token".to_string());
            return;
        }

        let tokens = parse_refresh_tokens(&input);
        if tokens.is_empty() {
            status.set(AddAccountStatus::Error);
            message.set("No valid refresh tokens found (should start with 1//)".to_string());
            return;
        }

        status.set(AddAccountStatus::Loading);
        message.set(format!("Adding {} account(s)...", tokens.len()));

        spawn_local(async move {
            match commands::add_accounts_by_token(tokens).await {
                Ok((success, fail)) => {
                    if success > 0 {
                        status.set(AddAccountStatus::Success);
                        if fail > 0 {
                            message.set(format!("Added {} accounts, {} failed", success, fail));
                        } else {
                            message.set(format!("Successfully added {} account(s)!", success));
                        }
                        on_account_added.run(());
                        spawn_local(async move {
                            gloo_timers::future::TimeoutFuture::new(2000).await;
                            is_open.set(false);
                        });
                    } else {
                        status.set(AddAccountStatus::Error);
                        message.set(format!("All {} token(s) failed to add", fail));
                    }
                }
                Err(e) => {
                    status.set(AddAccountStatus::Error);
                    message.set(format!("Error: {}", e));
                }
            }
        });
    };

    view! {
        <div class="token-tab">
            <label class="token-label">"Refresh Token"</label>
            <textarea
                class="token-input"
                placeholder="Paste your refresh token here (starts with 1//...)"
                prop:value=move || refresh_token_input.get()
                on:input=move |ev| refresh_token_input.set(event_target_value(&ev))
            />
            <p class="token-hint">"You can add multiple tokens at once (one per line or JSON array)"</p>
            <button
                class="btn btn--primary btn--full"
                on:click=on_submit_token
                disabled=move || matches!(status.get(), AddAccountStatus::Loading | AddAccountStatus::Success)
            >
                "Add Account"
            </button>
        </div>
    }
}

#[component]
pub fn ImportTab() -> impl IntoView {
    view! {
        <div class="import-tab">
            <h3>"Import from Local DB"</h3>
            <p>"Import accounts from your local VSCode/Cursor installation."</p>
            <button class="btn btn--secondary btn--full" disabled=true>
                "🗃️ Import from Local DB (Not available in browser)"
            </button>
        </div>
    }
}
