use crate::api::commands;
use crate::api_models::Account;
use crate::app::AppState;
use crate::components::{AccountDetailsModal, AddAccountModal, Modal, ModalType};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn Modals(
    delete_confirm: RwSignal<Option<String>>,
    batch_delete_confirm: RwSignal<bool>,
    toggle_proxy_confirm: RwSignal<Option<(String, bool)>>,
    warmup_confirm: RwSignal<bool>,
    add_account_modal_open: RwSignal<bool>,
    details_account: RwSignal<Option<Account>>,
    selected_count: Memo<usize>,
    execute_delete: impl Fn() + Send + Sync + 'static + Clone,
    execute_batch_delete: impl Fn() + Send + Sync + 'static + Clone,
    execute_toggle_proxy: impl Fn() + Send + Sync + 'static + Clone,
    on_warmup_all: impl Fn() + Send + Sync + 'static + Clone,
    state: AppState,
) -> impl IntoView {
    view! {
        <Modal
            is_open=Signal::derive(move || delete_confirm.get().is_some())
            title="Delete Account".to_string()
            message="Are you sure you want to delete this account?".to_string()
            modal_type=ModalType::Danger
            confirm_text="Delete".to_string()
            on_confirm=Callback::new(move |_| execute_delete())
            on_cancel=Callback::new(move |_| delete_confirm.set(None))
        />

        <Modal
            is_open=Signal::derive(move || batch_delete_confirm.get())
            title="Delete Selected Accounts".to_string()
            message=format!("Delete {} selected accounts?", selected_count.get())
            modal_type=ModalType::Danger
            confirm_text="Delete All".to_string()
            on_confirm=Callback::new(move |_| execute_batch_delete())
            on_cancel=Callback::new(move |_| batch_delete_confirm.set(false))
        />

        <Modal
            is_open=Signal::derive(move || toggle_proxy_confirm.get().is_some())
            title="Toggle Proxy".to_string()
            message="Toggle proxy status for this account?".to_string()
            modal_type=ModalType::Confirm
            on_confirm=Callback::new(move |_| execute_toggle_proxy())
            on_cancel=Callback::new(move |_| toggle_proxy_confirm.set(None))
        />

        <AddAccountModal
            is_open=add_account_modal_open
            on_account_added=Callback::new({
                let s = state.clone();
                move |_| {
                    let s = s.clone();
                    spawn_local(async move {
                        if let Ok(accounts) = commands::list_accounts().await {
                            s.accounts.set(accounts);
                        }
                    });
                }
            })
        />

        <Modal
            is_open=Signal::derive(move || warmup_confirm.get())
            title="Warmup All Accounts".to_string()
            message="This will send warmup requests for all accounts. Continue?".to_string()
            modal_type=ModalType::Confirm
            confirm_text="Warmup".to_string()
            on_confirm=Callback::new(move |_| on_warmup_all())
            on_cancel=Callback::new(move |_| warmup_confirm.set(false))
        />

        <AccountDetailsModal
            account=Signal::derive(move || details_account.get())
            on_close=Callback::new(move |_| details_account.set(None))
        />
    }
}
