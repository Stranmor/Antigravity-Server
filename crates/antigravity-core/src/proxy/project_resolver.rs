use serde_json::Value;

/// Use Antigravity loadCodeAssist API to get project_id
/// This is the correct method to get cloudaicompanionProject
pub async fn fetch_project_id(access_token: &str) -> Result<String, String> {
    fetch_project_id_with_proxy(access_token, None).await
}

/// Use Antigravity loadCodeAssist API to get project_id, routing through optional proxy.
pub async fn fetch_project_id_with_proxy(
    access_token: &str,
    proxy_url: Option<&str>,
) -> Result<String, String> {
    let url = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";

    let request_body = serde_json::json!({
        "metadata": {
            "ideType": "ANTIGRAVITY"
        }
    });

    let client = crate::utils::http::create_client_for_account(30, proxy_url, false)?;
    let response = client
        .post(url)
        .bearer_auth(access_token)
        .header("Host", "cloudcode-pa.googleapis.com")
        .header("User-Agent", crate::proxy::upstream::user_agent::default_user_agent())
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("loadCodeAssist Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("loadCodeAssist returnerror {}: {}", status, body));
    }

    let data: Value = response.json().await.map_err(|e| format!("parseResponse failed: {}", e))?;

    // extract cloudaicompanionProject
    if let Some(project_id) = data.get("cloudaicompanionProject").and_then(|v| v.as_str()) {
        if !project_id.is_empty() {
            return Ok(project_id.to_string());
        }
    }

    // Google removed cloudaicompanionProject from loadCodeAssist response.
    // Use the known-good default project ID (same as upstream and quota.rs fallback).
    let default_pid = "bamboo-precept-lgxtn";
    tracing::info!(
        "Account has no cloudaicompanionProject, using default project: {}",
        default_pid
    );
    Ok(default_pid.to_string())
}
