//! HTTP client and vision chat completion for ZAI API.

use serde_json::{json, Value};
use tokio::time::Duration;

use crate::proxy::config::UpstreamProxyConfig;

use super::media::user_message_with_content;

pub const ZAI_PAAZ_CHAT_COMPLETIONS_URL: &str = "https://api.z.ai/api/paas/v4/chat/completions";

/// Build HTTP client with optional upstream proxy.
pub fn build_client(
    upstream_proxy: UpstreamProxyConfig,
    timeout_secs: u64,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(timeout_secs.max(5)));

    if upstream_proxy.enabled && !upstream_proxy.url.is_empty() {
        let proxy = reqwest::Proxy::all(&upstream_proxy.url)
            .map_err(|e| format!("Invalid upstream proxy url: {}", e))?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

/// Execute vision chat completion request to ZAI API.
pub async fn vision_chat_completion(
    client: &reqwest::Client,
    api_key: &str,
    system_prompt: &str,
    user_content: Vec<Value>,
    prompt: &str,
) -> Result<String, String> {
    let body = json!({
        "model": "glm-4.6v",
        "messages": [
            { "role": "system", "content": system_prompt },
            user_message_with_content(user_content, prompt),
        ],
        "thinking": { "type": "enabled" },
        "stream": false,
        "temperature": 0.8,
        "top_p": 0.6,
        "max_tokens": 32768
    });

    let resp = client
        .post(ZAI_PAAZ_CHAT_COMPLETIONS_URL)
        .bearer_auth(api_key)
        .header("X-Title", "Vision MCP Local")
        .header("Accept-Language", "en-US,en")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Upstream request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, text));
    }

    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid JSON response: {}", e))?;
    let content = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| "Invalid API response: missing choices[0].message.content".to_string())?;

    Ok(content.to_string())
}
