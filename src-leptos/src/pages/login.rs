//! Login page for API key authentication

use crate::api::auth::{is_authenticated, set_stored_api_key};
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use wasm_bindgen::JsCast;

#[component]
pub fn Login() -> impl IntoView {
    let api_key = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let loading = RwSignal::new(false);
    let checking = RwSignal::new(is_authenticated());
    let navigate = use_navigate();

    let nav_for_check = navigate.clone();
    Effect::new(move |_| {
        if is_authenticated() {
            let nav = nav_for_check.clone();
            leptos::task::spawn_local(async move {
                match crate::api::commands::get_status().await {
                    Ok(_) => nav("/", Default::default()),
                    Err(_) => {
                        crate::api::auth::clear_stored_api_key();
                        checking.set(false);
                    },
                }
            });
        }
    });

    let nav_for_submit = navigate.clone();
    let do_submit = move || {
        let key = api_key.get();
        if key.trim().is_empty() {
            error.set(Some("API key is required".to_string()));
            return;
        }

        loading.set(true);
        error.set(None);

        set_stored_api_key(&key);

        let nav = nav_for_submit.clone();
        leptos::task::spawn_local(async move {
            match crate::api::commands::get_status().await {
                Ok(_) => {
                    nav("/", Default::default());
                },
                Err(e) => {
                    crate::api::auth::clear_stored_api_key();
                    if e.contains("Unauthorized") || e.contains("401") {
                        error.set(Some("Invalid API key".to_string()));
                    } else {
                        error.set(Some(format!("Connection failed: {}", e)));
                    }
                    loading.set(false);
                },
            }
        });
    };

    let on_input = move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let input: web_sys::HtmlInputElement = target.dyn_into().unwrap();
        api_key.set(input.value());
    };

    let submit_for_button = do_submit.clone();
    let on_button_click = move || {
        submit_for_button();
    };

    view! {
        <Show when=move || checking.get() fallback={
            let do_submit = do_submit.clone();
            move || {
                let submit_for_keydown = do_submit.clone();
                let on_keydown = move |ev: web_sys::KeyboardEvent| {
                    if ev.key() == "Enter" {
                        submit_for_keydown();
                    }
                };
                view! {
                    <div class="login-page">
                        <div class="login-container">
                            <div class="login-header">
                                <img src="/icon.png" alt="Antigravity" class="login-logo" />
                                <h1>"Antigravity Manager"</h1>
                                <p class="login-subtitle">"Enter your API key to continue"</p>
                            </div>

                            <Show when=move || error.get().is_some()>
                                <div class="alert alert--error">
                                    <span>{move || error.get().unwrap_or_default()}</span>
                                </div>
                            </Show>

                            <div class="login-form">
                                <div class="form-group">
                                    <label for="api-key">"API Key"</label>
                                    <input
                                        id="api-key"
                                        type="password"
                                        placeholder="sk-ag-..."
                                        class="form-input"
                                        prop:value=move || api_key.get()
                                        on:input=on_input
                                        on:keydown=on_keydown
                                        disabled=move || loading.get()
                                    />
                                </div>

                                <Button
                                    text="Login".to_string()
                                    variant=ButtonVariant::Primary
                                    loading=loading.get()
                                    on_click=on_button_click.clone()
                                    class="btn--full-width"
                                />
                            </div>

                            <p class="login-hint">
                                "Find your API key in Settings → API Proxy"
                            </p>
                        </div>
                    </div>
                }
            }
        }>
            <div class="login-page">
                <div class="login-container">
                    <p>"Checking authentication..."</p>
                </div>
            </div>
        </Show>
    }
}
