//! Monitor page - Real-time request logging with detailed view

use crate::api::commands;
use crate::api_models::{ProxyRequestLog, ProxyStats};
use crate::app::AppState;
use crate::components::{Button, ButtonVariant};
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn Monitor() -> impl IntoView {
    let state = expect_context::<AppState>();

    // Local state
    let logs = RwSignal::new(Vec::<ProxyRequestLog>::new());
    let stats = RwSignal::new(ProxyStats::default());
    let filter = RwSignal::new(String::new());
    let logging_enabled = RwSignal::new(true);
    let loading = RwSignal::new(false);

    // Selected log for detail view
    let selected_log = RwSignal::new(Option::<ProxyRequestLog>::None);

    // Load logs and stats
    let load_data = move || {
        loading.set(true);
        spawn_local(async move {
            // Load logs
            if let Ok(new_logs) = commands::get_proxy_logs(Some(100)).await {
                logs.set(new_logs);
            }
            // Load stats
            if let Ok(new_stats) = commands::get_proxy_stats().await {
                stats.set(new_stats);
            }
            loading.set(false);
        });
    };

    // Initial load
    Effect::new(move |_| {
        load_data();
    });

    // Auto-refresh every 2 seconds when enabled
    Effect::new(move |_| {
        if logging_enabled.get() && state.proxy_status.get().running {
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(2000).await;
                if logging_enabled.get() {
                    load_data();
                }
            });
        }
    });

    // Filtered logs
    let filtered_logs = Memo::new(move |_| {
        let query = filter.get().to_lowercase();
        let all_logs = logs.get();

        if query.is_empty() {
            all_logs
        } else {
            all_logs
                .into_iter()
                .filter(|l| {
                    l.url.to_lowercase().contains(&query)
                        || l.method.to_lowercase().contains(&query)
                        || l.model
                            .as_ref()
                            .is_some_and(|m| m.to_lowercase().contains(&query))
                        || l.status.to_string().contains(&query)
                })
                .collect()
        }
    });

    // Computed stats from logs
    let log_stats = Memo::new(move |_| {
        let s = stats.get();
        (s.total_requests, s.success_count, s.error_count)
    });

    let on_clear = move || {
        spawn_local(async move {
            if commands::clear_proxy_logs().await.is_ok() {
                logs.set(vec![]);
            }
        });
    };

    let on_toggle_logging = move || {
        let new_state = !logging_enabled.get();
        spawn_local(async move {
            if commands::set_proxy_monitor_enabled(new_state).await.is_ok() {
                logging_enabled.set(new_state);
            }
        });
    };

    view! {
        <div class="page monitor">
            <header class="page-header">
                <div class="header-left">
                    <a href="/proxy" class="back-button">"← Back"</a>
                    <div>
                        <h1>"Request Monitor"</h1>
                        <p class="subtitle">"Real-time API request logging"</p>
                    </div>
                </div>

                <div class="header-stats">
                    <span class="stat stat--total">{move || log_stats.get().0}" REQS"</span>
                    <span class="stat stat--success">{move || log_stats.get().1}" OK"</span>
                    <span class="stat stat--error">{move || log_stats.get().2}" ERR"</span>
                </div>
            </header>

            // Proxy status banner
            <Show when=move || !state.proxy_status.get().running>
                <div class="alert alert--warning">
                    <span class="alert-icon">"⚠️"</span>
                    <span>"Proxy is not running. Start it from the API Proxy page to see requests."</span>
                    <a href="/proxy" class="btn btn--primary btn--sm">"Start Proxy"</a>
                </div>
            </Show>

            // Controls
            <div class="monitor-controls">
                <button
                    class=move || format!("recording-btn {}", if logging_enabled.get() { "recording" } else { "paused" })
                    on:click=move |_| on_toggle_logging()
                >
                    <span class="dot"></span>
                    {move || if logging_enabled.get() { "Recording" } else { "Paused" }}
                </button>

                <div class="search-box">
                    <input
                        type="text"
                        placeholder="Filter by URL, model, status..."
                        prop:value=move || filter.get()
                        on:input=move |ev| filter.set(event_target_value(&ev))
                    />
                </div>

                <div class="quick-filters">
                    <button
                        class=move || if filter.get().is_empty() { "active" } else { "" }
                        on:click=move |_| filter.set(String::new())
                    >"All"</button>
                    <button
                        class=move || if filter.get() == "4" { "active" } else { "" }
                        on:click=move |_| filter.set("4".to_string())
                    >"Errors"</button>
                    <button
                        class=move || if filter.get() == "gemini" { "active" } else { "" }
                        on:click=move |_| filter.set("gemini".to_string())
                    >"Gemini"</button>
                    <button
                        class=move || if filter.get() == "claude" { "active" } else { "" }
                        on:click=move |_| filter.set("claude".to_string())
                    >"Claude"</button>
                    <button
                        class=move || if filter.get() == "openai" { "active" } else { "" }
                        on:click=move |_| filter.set("openai".to_string())
                    >"OpenAI"</button>
                </div>

                <div class="controls-right">
                    <Button
                        text="🔄".to_string()
                        variant=ButtonVariant::Ghost
                        loading=loading.get()
                        on_click=load_data
                    />
                    <Button
                        text="🗑".to_string()
                        variant=ButtonVariant::Ghost
                        on_click=on_clear
                    />
                </div>
            </div>

            // Token stats
            <div class="token-stats">
                <div class="token-stat">
                    <span class="token-label">"Input Tokens"</span>
                    <span class="token-value">{move || format_tokens(stats.get().total_input_tokens)}</span>
                </div>
                <div class="token-stat">
                    <span class="token-label">"Output Tokens"</span>
                    <span class="token-value">{move || format_tokens(stats.get().total_output_tokens)}</span>
                </div>
            </div>

            // Logs table
            <div class="logs-table-container">
                <table class="logs-table">
                    <thead>
                        <tr>
                            <th class="col-time">"Time"</th>
                            <th class="col-status">"Status"</th>
                            <th class="col-method">"Method"</th>
                            <th class="col-model">"Model"</th>
                            <th class="col-account">"Account"</th>
                            <th class="col-path">"Path"</th>
                            <th class="col-tokens">"Tokens"</th>
                            <th class="col-duration">"Duration"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || filtered_logs.get()
                            key=|log| log.id.clone()
                            children=move |log| {
                                let log_for_click = log.clone();
                                let status_class = if log.status >= 200 && log.status < 400 { "success" }
                                    else if log.status >= 400 && log.status < 500 { "warning" }
                                    else { "error" };
                                let model_display = log.model.clone().unwrap_or_else(|| "-".to_string());
                                let mapped = log.mapped_model.clone().filter(|m| Some(m) != log.model.as_ref());
                                let account = log.account_email.clone()
                                    .map(|e| e.split('@').next().unwrap_or(&e).to_string())
                                    .unwrap_or_else(|| "-".to_string());
                                let tokens_in = log.input_tokens.unwrap_or(0);
                                let tokens_out = log.output_tokens.unwrap_or(0);
                                let time = format_timestamp(log.timestamp);
                                let has_error = log.error.is_some();
                                let has_details = log.request_body.is_some() || log.response_body.is_some() || log.error.is_some();

                                view! {
                                    <tr
                                        class=format!("log-row {} {}",
                                            if has_error { "has-error" } else { "" },
                                            if has_details { "clickable" } else { "" }
                                        )
                                        on:click=move |_| {
                                            if has_details {
                                                selected_log.set(Some(log_for_click.clone()));
                                            }
                                        }
                                    >
                                        <td class="col-time">{time}</td>
                                        <td class="col-status">
                                            <span class=format!("status-badge status-badge--{}", status_class)>
                                                {log.status}
                                            </span>
                                        </td>
                                        <td class="col-method">{log.method}</td>
                                        <td class="col-model">
                                            <span class="model-name">{model_display}</span>
                                            {mapped.map(|m| view! { <span class="model-mapped">" → "{m}</span> })}
                                        </td>
                                        <td class="col-account">{account}</td>
                                        <td class="col-path"><code>{log.url}</code></td>
                                        <td class="col-tokens">
                                            <span class="tokens-in">{tokens_in}</span>
                                            " / "
                                            <span class="tokens-out">{tokens_out}</span>
                                        </td>
                                        <td class="col-duration">{log.duration}"ms"</td>
                                    </tr>
                                }
                            }
                        />
                    </tbody>
                </table>

                <Show when=move || filtered_logs.get().is_empty()>
                    <div class="empty-state">
                        <span class="empty-icon">"📡"</span>
                        <p>{move || if state.proxy_status.get().running {
                            "No requests yet"
                        } else {
                            "Proxy is not running"
                        }}</p>
                        <p class="hint">{move || if state.proxy_status.get().running {
                            "Requests will appear here as they come in"
                        } else {
                            "Start the proxy to begin monitoring requests"
                        }}</p>
                    </div>
                </Show>
            </div>

            // Detail Modal
            <Show when=move || selected_log.get().is_some()>
                <LogDetailModal
                    log=selected_log.get().unwrap()
                    on_close=move || selected_log.set(None)
                />
            </Show>
        </div>
    }
}

/// Modal component for displaying request/response details
#[component]
fn LogDetailModal(log: ProxyRequestLog, on_close: impl Fn() + 'static + Clone) -> impl IntoView {
    let on_close_backdrop = on_close.clone();
    let on_close_button = on_close.clone();

    let status_class = if log.status >= 200 && log.status < 400 {
        "success"
    } else if log.status >= 400 && log.status < 500 {
        "warning"
    } else {
        "error"
    };

    // Pretty-print JSON if possible
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
                    // Metadata section
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

                    // Error section (if present)
                    {error_msg.map(|err| view! {
                        <section class="detail-section error-section">
                            <h3>"❌ Error"</h3>
                            <pre class="code-block error-block">{err}</pre>
                        </section>
                    })}

                    // Request body section
                    {request_body.map(|body| view! {
                        <section class="detail-section">
                            <h3>"📤 Request Body"</h3>
                            <pre class="code-block">{body}</pre>
                        </section>
                    })}

                    // Response body section
                    {response_body.map(|body| view! {
                        <section class="detail-section">
                            <h3>"📥 Response Body"</h3>
                            <pre class="code-block">{body}</pre>
                        </section>
                    })}

                    // No data message
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

fn format_timestamp(ts: i64) -> String {
    // Backend stores timestamp in MILLISECONDS, convert to seconds first
    let ts_secs = ts / 1000;
    let secs = ts_secs % 86400;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, s)
}

fn format_timestamp_full(ts: i64) -> String {
    // Backend stores timestamp in MILLISECONDS, convert to seconds first
    let ts_secs = ts / 1000;
    let days_since_epoch = ts_secs / 86400;
    let secs = ts_secs % 86400;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;

    // Simple date calculation (approximate, good enough for display)
    let year = 1970 + (days_since_epoch / 365);
    let day_of_year = days_since_epoch % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, hours, mins, s
    )
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}
