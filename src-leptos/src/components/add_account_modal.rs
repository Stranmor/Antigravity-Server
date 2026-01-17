//! Add Account Modal Component
//!
//! Modal dialog for adding accounts via OAuth with URL display/copy functionality.

use crate::api::commands;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum AddAccountTab {
    #[default]
    OAuth,
    Token,
    Import,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum AddAccountStatus {
    #[default]
    Idle,
    Loading,
    Success,
    Error,
}

#[component]
pub fn AddAccountModal(
    /// Signal controlling modal visibility
    is_open: RwSignal<bool>,
    /// Callback when account is added successfully
    on_account_added: Callback<()>,
) -> impl IntoView {
    let active_tab = RwSignal::new(AddAccountTab::OAuth);
    let status = RwSignal::new(AddAccountStatus::Idle);
    let message = RwSignal::new(String::new());
    let oauth_url = RwSignal::new(String::new());
    let url_copied = RwSignal::new(false);
    let refresh_token_input = RwSignal::new(String::new());

    // Fetch OAuth URL when modal opens on OAuth tab
    Effect::new(move |_| {
        if is_open.get()
            && matches!(active_tab.get(), AddAccountTab::OAuth)
            && oauth_url.get().is_empty()
        {
            spawn_local(async move {
                if let Ok(url) = commands::prepare_oauth_url().await {
                    oauth_url.set(url);
                }
            });
        }
    });

    // Reset state when modal closes
    Effect::new(move |_| {
        if !is_open.get() {
            status.set(AddAccountStatus::Idle);
            message.set(String::new());
            oauth_url.set(String::new());
            url_copied.set(false);
            refresh_token_input.set(String::new());
        }
    });

    let on_copy_url = move |_| {
        let url = oauth_url.get();
        if !url.is_empty()
            && let Some(window) = web_sys::window()
        {
            let clipboard = window.navigator().clipboard();
            let _ = clipboard.write_text(&url);
            url_copied.set(true);
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(1500).await;
                url_copied.set(false);
            });
        }
    };

    let on_open_url = move |_| {
        let url = oauth_url.get();
        if !url.is_empty()
            && let Some(window) = web_sys::window()
        {
            let _ = window.open_with_url_and_target(&url, "_blank");
        }
    };

    let on_start_oauth = move |_| {
        status.set(AddAccountStatus::Loading);
        message.set("Opening browser for OAuth...".to_string());

        let url = oauth_url.get();
        if !url.is_empty()
            && let Some(window) = web_sys::window()
        {
            let _ = window.open_with_url_and_target(&url, "_blank");
        }

        message.set("Complete authorization in your browser, then click 'Finish OAuth' or refresh the accounts list.".to_string());
    };

    let on_finish_oauth = move |_| {
        status.set(AddAccountStatus::Loading);
        message.set("Checking for new account...".to_string());

        spawn_local(async move {
            // Just refresh accounts list - the callback endpoint already created the account
            match commands::list_accounts().await {
                Ok(_) => {
                    status.set(AddAccountStatus::Success);
                    message.set("Account added! Refresh the list to see it.".to_string());
                    on_account_added.run(());

                    // Close modal after delay
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

    let on_submit_token = move |_| {
        let input = refresh_token_input.get();
        if input.trim().is_empty() {
            status.set(AddAccountStatus::Error);
            message.set("Please enter a refresh token".to_string());
            return;
        }

        // Parse tokens (single, multi-line, or JSON array)
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

                        // Close modal after delay
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

    fn parse_refresh_tokens(input: &str) -> Vec<String> {
        let input = input.trim();
        let mut tokens = Vec::new();

        // Try JSON array first
        if input.starts_with('[')
            && input.ends_with(']')
            && let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(input)
        {
            for item in parsed {
                // Check refresh_token field first
                if let Some(token) = item
                    .get("refresh_token")
                    .and_then(|v| v.as_str())
                    .filter(|t| t.starts_with("1//"))
                {
                    tokens.push(token.to_string());
                } else if let Some(token) = item.as_str().filter(|t| t.starts_with("1//")) {
                    tokens.push(token.to_string());
                }
            }
            if !tokens.is_empty() {
                return tokens;
            }
        }

        // Try multi-line (one token per line) or space-separated
        for line in input.lines() {
            for word in line.split_whitespace() {
                let word = word.trim_matches(|c: char| {
                    !c.is_alphanumeric() && c != '/' && c != '_' && c != '-'
                });
                if word.starts_with("1//") {
                    tokens.push(word.to_string());
                }
            }
        }

        // Deduplicate
        tokens.sort();
        tokens.dedup();
        tokens
    }

    let on_close = move |_| {
        is_open.set(false);
    };

    let status_class = move || match status.get() {
        AddAccountStatus::Idle => "",
        AddAccountStatus::Loading => "alert alert--info",
        AddAccountStatus::Success => "alert alert--success",
        AddAccountStatus::Error => "alert alert--error",
    };

    view! {
        <Show when=move || is_open.get()>
            <div class="modal-overlay" on:click=on_close>
                <div class="modal-content add-account-modal" on:click=|e| e.stop_propagation()>
                    <h2 class="modal-title">"Add Account"</h2>

                    // Tab navigation
                    <div class="tab-nav">
                        <button
                            class=move || if matches!(active_tab.get(), AddAccountTab::OAuth) { "tab-btn active" } else { "tab-btn" }
                            on:click=move |_| active_tab.set(AddAccountTab::OAuth)
                        >
                            "🌐 OAuth"
                        </button>
                        <button
                            class=move || if matches!(active_tab.get(), AddAccountTab::Token) { "tab-btn active" } else { "tab-btn" }
                            on:click=move |_| active_tab.set(AddAccountTab::Token)
                        >
                            "🔑 Token"
                        </button>
                        <button
                            class=move || if matches!(active_tab.get(), AddAccountTab::Import) { "tab-btn active" } else { "tab-btn" }
                            on:click=move |_| active_tab.set(AddAccountTab::Import)
                        >
                            "📥 Import"
                        </button>
                    </div>

                    // Status message
                    <Show when=move || !message.get().is_empty()>
                        <div class=status_class>
                            {move || message.get()}
                        </div>
                    </Show>

                    // Tab content
                    <div class="tab-content">
                        // OAuth Tab
                        <Show when=move || matches!(active_tab.get(), AddAccountTab::OAuth)>
                            <div class="oauth-tab">
                                <div class="oauth-icon">"🌐"</div>
                                <h3>"Google OAuth (Recommended)"</h3>
                                <p class="oauth-desc">
                                    "Authorize with your Google account to add it to Antigravity."
                                </p>

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
                                                if url.len() > 60 {
                                                    format!("{}...", &url[..60])
                                                } else {
                                                    url
                                                }
                                            }}</code>
                                            <button
                                                class="btn btn--small btn--secondary"
                                                on:click=on_copy_url
                                                title="Copy URL"
                                            >
                                                {move || if url_copied.get() { "✓ Copied" } else { "📋 Copy" }}
                                            </button>
                                            <button
                                                class="btn btn--small btn--secondary"
                                                on:click=on_open_url
                                                title="Open in browser"
                                            >
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
                        </Show>

                        // Token Tab
                        <Show when=move || matches!(active_tab.get(), AddAccountTab::Token)>
                            <div class="token-tab">
                                <label class="token-label">"Refresh Token"</label>
                                <textarea
                                    class="token-input"
                                    placeholder="Paste your refresh token here (starts with 1//...)"
                                    prop:value=move || refresh_token_input.get()
                                    on:input=move |ev| refresh_token_input.set(event_target_value(&ev))
                                />
                                <p class="token-hint">
                                    "You can add multiple tokens at once (one per line or JSON array)"
                                </p>
                                <button
                                    class="btn btn--primary btn--full"
                                    on:click=on_submit_token
                                    disabled=move || matches!(status.get(), AddAccountStatus::Loading | AddAccountStatus::Success)
                                >
                                    "Add Account"
                                </button>
                            </div>
                        </Show>

                        // Import Tab
                        <Show when=move || matches!(active_tab.get(), AddAccountTab::Import)>
                            <div class="import-tab">
                                <h3>"Import from Local DB"</h3>
                                <p>"Import accounts from your local VSCode/Cursor installation."</p>
                                <button
                                    class="btn btn--secondary btn--full"
                                    disabled=true
                                >
                                    "🗃️ Import from Local DB (Not available in browser)"
                                </button>
                            </div>
                        </Show>
                    </div>

                    // Footer
                    <div class="modal-footer">
                        <button class="btn btn--secondary" on:click=on_close>
                            "Cancel"
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
