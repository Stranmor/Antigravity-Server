//! Modal dialog component

use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ModalType {
    #[default]
    Confirm,
    Alert,
    Danger,
}

#[component]
pub fn Modal(
    #[prop(into)] is_open: Signal<bool>,
    #[prop(into)] title: String,
    #[prop(optional)] message: Option<String>,
    #[prop(default = ModalType::Confirm)] modal_type: ModalType,
    #[prop(optional)] confirm_text: Option<String>,
    #[prop(optional)] cancel_text: Option<String>,
    #[prop(into)] on_confirm: Callback<()>,
    #[prop(into)] on_cancel: Callback<()>,
) -> impl IntoView {
    let confirm_text = confirm_text.unwrap_or_else(|| "Confirm".to_string());
    let cancel_text = cancel_text.unwrap_or_else(|| "Cancel".to_string());
    
    let confirm_class = match modal_type {
        ModalType::Danger => "btn btn--danger",
        _ => "btn btn--primary",
    };
    
    let on_cancel_overlay = on_cancel.clone();
    let on_cancel_close = on_cancel.clone();
    let on_cancel_btn = on_cancel.clone();
    let title_clone = title.clone();
    let message_clone = message.clone();
    let confirm_text_clone = confirm_text.clone();
    let cancel_text_clone = cancel_text.clone();

    view! {
        <Show when=move || is_open.get()>
            <div class="modal-overlay" on:click=move |_| on_cancel_overlay.run(())>
                <div class="modal" on:click=|e| e.stop_propagation()>
                    <div class="modal-header">
                        <h3 class="modal-title">{title_clone.clone()}</h3>
                        <button class="modal-close" on:click=move |_| on_cancel_close.run(())>
                            "×"
                        </button>
                    </div>
                    
                    <div class="modal-body">
                        {message_clone.clone().map(|msg| view! { <p>{msg}</p> })}
                    </div>
                    
                    <div class="modal-footer">
                        <button 
                            class="btn btn--secondary"
                            on:click=move |_| on_cancel_btn.run(())
                        >
                            {cancel_text_clone.clone()}
                        </button>
                        <button 
                            class=confirm_class
                            on:click=move |_| on_confirm.run(())
                        >
                            {confirm_text_clone.clone()}
                        </button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
