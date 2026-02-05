use crate::app::AppState;
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;

/// Page header with action buttons.
#[component]
pub(crate) fn Header(
    state: AppState,
    sync_pending: RwSignal<bool>,
    refresh_pending: RwSignal<bool>,
    warmup_pending: RwSignal<bool>,
    warmup_confirm: RwSignal<bool>,
    add_account_modal_open: RwSignal<bool>,
    on_sync_local: impl Fn() + 'static + Clone,
    on_refresh_all_quotas: impl Fn() + 'static + Clone,
) -> impl IntoView {
    view! {
        <header class="page-header">
            <div class="header-left">
                <h1>"Accounts"</h1>
                <p class="subtitle">
                    {move || format!("{} accounts", state.accounts.get().len())}
                </p>
            </div>
            <div class="header-actions">
                <Button
                    text="📥 Sync".to_string()
                    variant=ButtonVariant::Secondary
                    on_click=on_sync_local
                    loading=sync_pending.get()
                />
                <Button
                    text="🔄 Refresh All".to_string()
                    variant=ButtonVariant::Secondary
                    on_click=on_refresh_all_quotas
                    loading=refresh_pending.get()
                />
                <Button
                    text="✨ Warmup All".to_string()
                    variant=ButtonVariant::Secondary
                    on_click=move || warmup_confirm.set(true)
                    loading=warmup_pending.get()
                />
                <Button
                    text="➕ Add".to_string()
                    variant=ButtonVariant::Primary
                    on_click=move || add_account_modal_open.set(true)
                    loading=false
                />
            </div>
        </header>
    }
}

/// Message banner for success/error notifications.
#[component]
pub(crate) fn MessageBanner(message: RwSignal<Option<(String, bool)>>) -> impl IntoView {
    view! {
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
    }
}
