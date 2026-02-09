//! Monitor page - Real-time request logging with detailed view

pub(crate) mod formatters;
pub(crate) mod log_detail;

use crate::api::commands;
use crate::api_models::{ProxyRequestLog, ProxyStats};
use crate::app::AppState;
use crate::components::{Button, ButtonVariant};
use formatters::{format_timestamp, format_tokens};
use gloo_timers::callback::Interval;
use leptos::prelude::*;
use leptos::task::spawn_local;
use log_detail::LogDetailModal;
use std::cell::RefCell;
use std::rc::Rc;

/// Monitor page for real-time request logging.
#[component]
pub(crate) fn Monitor() -> impl IntoView {
    let state = expect_context::<AppState>();

    let logs = RwSignal::new(Vec::<ProxyRequestLog>::new());
    let stats = RwSignal::new(ProxyStats::default());
    let filter = RwSignal::new(String::new());
    let logging_enabled = RwSignal::new(true);
    let loading = RwSignal::new(false);
    let selected_log = RwSignal::new(Option::<ProxyRequestLog>::None);

    let load_data = move |show_loading: bool| {
        if show_loading {
            loading.set(true);
        }
        spawn_local(async move {
            if let Ok(new_logs) = commands::get_proxy_logs(Some(100)).await {
                logs.set(new_logs);
            }
            if let Ok(new_stats) = commands::get_proxy_stats().await {
                stats.set(new_stats);
            }
            if show_loading {
                loading.set(false);
            }
        });
    };

    Effect::new(move |_| {
        load_data(true);
    });

    let poller = Rc::new(RefCell::new(None::<Interval>));
    Effect::new(move |_| {
        let should_poll = logging_enabled.get() && state.proxy_status.get().running;
        if should_poll {
            if poller.borrow().is_none() {
                load_data(false);
                let poller_ref = poller.clone();
                let interval = Interval::new(2000, move || {
                    if logging_enabled.get() && state.proxy_status.get().running {
                        load_data(false);
                    }
                });
                *poller_ref.borrow_mut() = Some(interval);
            }
        } else {
            poller.borrow_mut().take();
        }
    });

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
                        || l.model.as_ref().is_some_and(|m| m.to_lowercase().contains(&query))
                        || l.status.to_string().contains(&query)
                })
                .collect()
        }
    });

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

            <Show when=move || !state.proxy_status.get().running>
                <div class="alert alert--warning">
                    <span class="alert-icon">"⚠️"</span>
                    <span>"Proxy is not running. Start it from the API Proxy page to see requests."</span>
                    <a href="/proxy" class="btn btn--primary btn--sm">"Start Proxy"</a>
                </div>
            </Show>

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
                        on_click=move || load_data(true)
                    />
                    <Button
                        text="🗑".to_string()
                        variant=ButtonVariant::Ghost
                        on_click=on_clear
                    />
                </div>
            </div>

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
                                let has_details = true;

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

            <Show when=move || selected_log.get().is_some()>
                {move || {
                    let Some(log) = selected_log.get() else {
                        return view! { <div></div> }.into_any();
                    };
                    view! {
                        <LogDetailModal
                            log=log
                            on_close=move || selected_log.set(None)
                        />
                    }.into_any()
                }}
            </Show>
        </div>
    }
}
