//! Account Details Modal Component
//!
//! Displays detailed quota information for an account with model-by-model breakdown.

use crate::api_models::Account;
use crate::formatters::{format_time_remaining, get_time_remaining_color};
use leptos::prelude::*;

#[component]
pub(crate) fn AccountDetailsModal(
    /// The account to display details for (None = closed)
    account: Signal<Option<Account>>,
    /// Callback to close the modal
    on_close: Callback<()>,
) -> impl IntoView {
    let quota_progress_class = |percentage: i32| -> &'static str {
        match percentage {
            0..=20 => "quota-fill--critical",
            21..=50 => "quota-fill--warning",
            _ => "quota-fill--good",
        }
    };

    let reset_time_class = |color: &str| -> &'static str {
        match color {
            "success" => "reset-time--success",
            "warning" => "reset-time--warning",
            _ => "reset-time--neutral",
        }
    };

    let on_backdrop_click = move |_| {
        on_close.run(());
    };

    view! {
        <Show when=move || account.get().is_some()>
            {move || {
                let Some(acc) = account.get() else {
                    return view! { <div></div> }.into_any();
                };
                let email = acc.email.clone();
                let models = acc.quota.as_ref().map(|q| q.models.clone()).unwrap_or_default();

                view! {
                    <div class="modal-overlay" on:click=on_backdrop_click>
                        <div class="modal-content account-details-modal" on:click=|e| e.stop_propagation()>
                            <header class="modal-header">
                                <div class="modal-header-left">
                                    <h2 class="modal-title">"Account Details"</h2>
                                    <span class="modal-subtitle">{email}</span>
                                </div>
                                <button
                                    class="btn btn--icon btn--close"
                                    on:click=move |_| on_close.run(())
                                >"×"</button>
                            </header>

                            <div class="modal-body">
                                {if models.is_empty() {
                                    view! {
                                        <div class="empty-state">
                                            <span class="empty-icon">"📊"</span>
                                            <p>"No quota data available"</p>
                                            <p class="empty-hint">"Try refreshing the account to fetch quota information"</p>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="model-quota-grid">
                                            {models.iter().map(|model| {
                                                let percentage = model.percentage;
                                                let name = model.name.clone();
                                                let reset_formatted = format_time_remaining(&model.reset_time);
                                                let reset_color = reset_time_class(get_time_remaining_color(&model.reset_time));
                                                let progress_class = quota_progress_class(percentage);

                                                view! {
                                                    <div class="model-quota-card">
                                                        <div class="model-header">
                                                            <span class="model-name">{name}</span>
                                                            <span class=format!("model-percentage {}", progress_class)>
                                                                {percentage}"%"
                                                            </span>
                                                        </div>
                                                        <div class="model-progress">
                                                            <div
                                                                class=format!("model-progress-bar {}", progress_class)
                                                                style=format!("width: {}%", percentage)
                                                            ></div>
                                                        </div>
                                                        <div class="model-reset">
                                                            <span class="reset-icon">"⏱"</span>
                                                            <span class=format!("reset-text {}", reset_color)>{reset_formatted}</span>
                                                        </div>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }}
                            </div>

                            <footer class="modal-footer">
                                <button
                                    class="btn btn--secondary"
                                    on:click=move |_| on_close.run(())
                                >"Close"</button>
                            </footer>
                        </div>
                    </div>
                }.into_any()
            }}
        </Show>
    }
}
