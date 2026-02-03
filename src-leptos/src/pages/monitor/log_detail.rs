//! Log detail modal component

use super::formatters::format_timestamp_full;
use crate::api_models::ProxyRequestLog;
use leptos::prelude::*;

#[component]
pub fn LogDetailModal(
    log: ProxyRequestLog,
    on_close: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let on_close_backdrop = on_close.clone();
    let on_close_button = on_close.clone();

    let status_class = if log.status >= 200 && log.status < 400 {
        "success"
    } else if log.status >= 400 && log.status < 500 {
        "warning"
    } else {
        "error"
    };

    let format_json = |s: &str| -> String {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
            serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| s.to_string())
        } else {
            s.to_string()
        }
    };

    let request_body = log.request_body.clone().map(|b| format_json(&b));
    let response_body = log.response_body.clone().map(|b| format_json(&b));
    let error_msg = log.error.clone();

    view! {
        <div class="modal-backdrop" on:click=move |_| on_close_backdrop()>
            <div class="modal log-detail-modal" on:click=|e| e.stop_propagation()>
                <header class="modal-header">
                    <div class="modal-title">
                        <span class=format!("status-badge status-badge--{}", status_class)>
                            {log.status}
                        </span>
                        <span class="method">{log.method.clone()}</span>
                        <code class="path">{log.url.clone()}</code>
                    </div>
                    <button class="modal-close" on:click=move |_| on_close_button()>"×"</button>
                </header>

                <div class="modal-body">
                    <section class="detail-section">
                        <h3>"Request Info"</h3>
                        <div class="detail-grid">
                            <div class="detail-item">
                                <span class="label">"ID"</span>
                                <code class="value">{log.id.clone()}</code>
                            </div>
                            <div class="detail-item">
                                <span class="label">"Time"</span>
                                <span class="value">{format_timestamp_full(log.timestamp)}</span>
                            </div>
                            <div class="detail-item">
                                <span class="label">"Duration"</span>
                                <span class="value">{log.duration}" ms"</span>
                            </div>
                            {log.model.clone().map(|m| view! {
                                <div class="detail-item">
                                    <span class="label">"Model"</span>
                                    <span class="value">{m}</span>
                                </div>
                            })}
                            {log.mapped_model.clone().map(|m| view! {
                                <div class="detail-item">
                                    <span class="label">"Mapped Model"</span>
                                    <span class="value">{m}</span>
                                </div>
                            })}
                            {log.mapping_reason.clone().map(|r| view! {
                                <div class="detail-item">
                                    <span class="label">"Mapping Reason"</span>
                                    <span class="value">{r}</span>
                                </div>
                            })}
                            {log.account_email.clone().map(|e| view! {
                                <div class="detail-item">
                                    <span class="label">"Account"</span>
                                    <span class="value">{e}</span>
                                </div>
                            })}
                            {log.input_tokens.map(|t| view! {
                                <div class="detail-item">
                                    <span class="label">"Input Tokens"</span>
                                    <span class="value">{t}</span>
                                </div>
                            })}
                            {log.output_tokens.map(|t| view! {
                                <div class="detail-item">
                                    <span class="label">"Output Tokens"</span>
                                    <span class="value">{t}</span>
                                </div>
                            })}
                        </div>
                    </section>

                    {error_msg.map(|err| view! {
                        <section class="detail-section error-section">
                            <h3>"❌ Error"</h3>
                            <pre class="code-block error-block">{err}</pre>
                        </section>
                    })}

                    {request_body.map(|body| view! {
                        <section class="detail-section">
                            <h3>"📤 Request Body"</h3>
                            <pre class="code-block">{body}</pre>
                        </section>
                    })}

                    {response_body.map(|body| view! {
                        <section class="detail-section">
                            <h3>"📥 Response Body"</h3>
                            <pre class="code-block">{body}</pre>
                        </section>
                    })}

                    <Show when=move || log.request_body.is_none() && log.response_body.is_none() && log.error.is_none()>
                        <div class="empty-detail">
                            <p>"No request/response data available for this log entry."</p>
                        </div>
                    </Show>
                </div>
            </div>
        </div>
    }
}
