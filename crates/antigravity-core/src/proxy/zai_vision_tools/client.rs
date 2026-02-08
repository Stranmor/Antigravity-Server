//! HTTP client and vision chat completion for ZAI API.

use serde_json::{json, Value};

use super::media::user_message_with_content;

pub use crate::proxy::common::client_builder::build_http_client;

pub const ZAI_PAAZ_CHAT_COMPLETIONS_URL: &str = "https://api.z.ai/api/paas/v4/chat/completions";

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

    let v: Value = resp.json().await.map_err(|e| format!("Invalid JSON response: {}", e))?;
    let content = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| "Invalid API response: missing choices[0].message.content".to_string())?;

    Ok(content.to_string())
}
