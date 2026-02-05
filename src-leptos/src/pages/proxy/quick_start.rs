use super::actions::on_copy;
use crate::api_models::Protocol;
use leptos::prelude::*;

/// Quick start code examples for different protocols.
#[component]
pub(crate) fn QuickStart(
    port: RwSignal<u16>,
    api_key: RwSignal<String>,
    selected_protocol: RwSignal<Protocol>,
    selected_model: RwSignal<String>,
    copied: RwSignal<Option<String>>,
) -> impl IntoView {
    let models = [
        ("gemini-3-flash", "Gemini 3 Flash"),
        ("gemini-3-pro-high", "Gemini 3 Pro"),
        ("claude-sonnet-4-5", "Claude Sonnet 4.5"),
        ("gemini-2.5-flash", "Gemini 2.5 Flash"),
    ];

    let get_example = move || {
        let p = port.get();
        let key = api_key.get();
        let model = selected_model.get();
        let base_url = format!("http://127.0.0.1:{}/v1", p);

        match selected_protocol.get() {
            Protocol::OpenAI => format!(
                r#"from openai import OpenAI

client = OpenAI(
    base_url="{}",
    api_key="{}"
)

response = client.chat.completions.create(
    model="{}",
    messages=[{{"role": "user", "content": "Hello"}}]
)

print(response.choices[0].message.content)"#,
                base_url, key, model
            ),
            Protocol::Anthropic => format!(
                r#"from anthropic import Anthropic

client = Anthropic(
    base_url="http://127.0.0.1:{}",
    api_key="{}"
)

response = client.messages.create(
    model="{}",
    max_tokens=1024,
    messages=[{{"role": "user", "content": "Hello"}}]
)

print(response.content[0].text)"#,
                p, key, model
            ),
            Protocol::Gemini => format!(
                r#"import google.generativeai as genai

genai.configure(
    api_key="{}",
    transport='rest',
    client_options={{'api_endpoint': 'http://127.0.0.1:{}'}}
)

model = genai.GenerativeModel('{}')
response = model.generate_content("Hello")
print(response.text)"#,
                key, p, model
            ),
        }
    };

    view! {
        <div class="config-card">
            <div class="config-header">
                <h2>"🚀 Quick Start"</h2>
            </div>

            <div class="quick-start">
                <div class="protocol-tabs">
                    <button
                        class=move || if matches!(selected_protocol.get(), Protocol::OpenAI) { "active" } else { "" }
                        on:click=move |_| selected_protocol.set(Protocol::OpenAI)
                    >"OpenAI"</button>
                    <button
                        class=move || if matches!(selected_protocol.get(), Protocol::Anthropic) { "active" } else { "" }
                        on:click=move |_| selected_protocol.set(Protocol::Anthropic)
                    >"Anthropic"</button>
                    <button
                        class=move || if matches!(selected_protocol.get(), Protocol::Gemini) { "active" } else { "" }
                        on:click=move |_| selected_protocol.set(Protocol::Gemini)
                    >"Gemini"</button>
                </div>

                <div class="model-selector">
                    <label>"Model:"</label>
                    <select
                        prop:value=move || selected_model.get()
                        on:change=move |ev| selected_model.set(event_target_value(&ev))
                    >
                        {models.iter().map(|(id, name)| {
                            view! { <option value=*id>{*name}</option> }
                        }).collect_view()}
                    </select>
                </div>

                <div class="code-block">
                    <div class="code-header">
                        <span>"Python"</span>
                        <button
                            class="btn btn--icon btn--sm"
                            on:click=move |_| {
                                let code = get_example();
                                on_copy(code, "code".to_string(), copied);
                            }
                        >
                            {move || if copied.get() == Some("code".to_string()) { "✓" } else { "Copy" }}
                        </button>
                    </div>
                    <pre class="code">{get_example}</pre>
                </div>

                <div class="base-url">
                    <label>"Base URL:"</label>
                    <code>{move || format!("http://127.0.0.1:{}/v1", port.get())}</code>
                </div>
            </div>
        </div>
    }
}
