//! Add Account Modal Component

mod parser;
mod tabs;

use tabs::{ImportTab, OAuthTab, TokenTab};

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
pub fn AddAccountModal(is_open: RwSignal<bool>, on_account_added: Callback<()>) -> impl IntoView {
    let active_tab = RwSignal::new(AddAccountTab::OAuth);
    let status = RwSignal::new(AddAccountStatus::Idle);
    let message = RwSignal::new(String::new());
    let oauth_url = RwSignal::new(String::new());
    let url_copied = RwSignal::new(false);
    let refresh_token_input = RwSignal::new(String::new());

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

    Effect::new(move |_| {
        if !is_open.get() {
            status.set(AddAccountStatus::Idle);
            message.set(String::new());
            oauth_url.set(String::new());
            url_copied.set(false);
            refresh_token_input.set(String::new());
        }
    });

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

                    <Show when=move || !message.get().is_empty()>
                        <div class=status_class>
                            {move || message.get()}
                        </div>
                    </Show>

                    <div class="tab-content">
                        <Show when=move || matches!(active_tab.get(), AddAccountTab::OAuth)>
                            <OAuthTab
                                status=status
                                message=message
                                oauth_url=oauth_url
                                url_copied=url_copied
                                is_open=is_open
                                on_account_added=on_account_added
                            />
                        </Show>

                        <Show when=move || matches!(active_tab.get(), AddAccountTab::Token)>
                            <TokenTab
                                status=status
                                message=message
                                refresh_token_input=refresh_token_input
                                is_open=is_open
                                on_account_added=on_account_added
                            />
                        </Show>

                        <Show when=move || matches!(active_tab.get(), AddAccountTab::Import)>
                            <ImportTab />
                        </Show>
                    </div>

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
