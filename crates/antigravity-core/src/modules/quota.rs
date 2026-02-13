//! Quota fetching and management for Antigravity accounts.
#![allow(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "percentage calculation: f64 * 100.0 -> i32"
)]

use crate::models::QuotaData;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::json;

const QUOTA_API_URL: &str = "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels";

fn quota_user_agent() -> String {
    format!(
        "antigravity/{} Darwin/arm64",
        crate::proxy::upstream::version_fetcher::get_current_version()
    )
}

#[derive(Debug, Serialize, Deserialize)]
struct QuotaResponse {
    models: std::collections::HashMap<String, ModelInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ModelInfo {
    #[serde(rename = "quotaInfo")]
    quota_info: Option<QuotaInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct QuotaInfo {
    #[serde(rename = "remainingFraction")]
    remaining_fraction: Option<f64>,
    #[serde(rename = "resetTime")]
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoadProjectResponse {
    #[serde(rename = "cloudaicompanionProject")]
    project_id: Option<String>,
    #[serde(rename = "currentTier")]
    current_tier: Option<Tier>,
    #[serde(rename = "paidTier")]
    paid_tier: Option<Tier>,
}

#[derive(Debug, Deserialize)]
struct Tier {
    id: Option<String>,
}

/// Create configured HTTP Client that routes through an account's proxy.
fn create_client_proxied(proxy_url: Option<&str>) -> Result<reqwest::Client, String> {
    crate::utils::http::create_client_for_account(15, proxy_url, false)
}

const CLOUD_CODE_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";

/// Get project ID and subscription type
async fn fetch_project_id(
    client: &reqwest::Client,
    access_token: &str,
    email: &str,
) -> (Option<String>, Option<String>) {
    let meta = json!({"metadata": {"ideType": "ANTIGRAVITY"}});

    let res = client
        .post(format!("{}/v1internal:loadCodeAssist", CLOUD_CODE_BASE_URL))
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", access_token))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, &quota_user_agent())
        .json(&meta)
        .send()
        .await;

    match res {
        Ok(res) => {
            if res.status().is_success() {
                match res.json::<LoadProjectResponse>().await {
                    Ok(data) => {
                        let project_id = data.project_id.clone();

                        // Core logic: prefer subscription ID from paid_tier, which better reflects actual account entitlements than current_tier
                        let subscription_tier = data
                            .paid_tier
                            .and_then(|t| t.id)
                            .or_else(|| data.current_tier.and_then(|t| t.id));

                        if let Some(ref tier) = subscription_tier {
                            crate::modules::logger::log_info(&format!(
                                "📊 [{}] Subscription identified: {}",
                                email, tier
                            ));
                        }

                        return (project_id, subscription_tier);
                    },
                    Err(e) => {
                        crate::modules::logger::log_warn(&format!(
                            "⚠️  [{}] loadCodeAssist parse error: {}",
                            email, e
                        ));
                    },
                }
            } else {
                crate::modules::logger::log_warn(&format!(
                    "⚠️  [{}] loadCodeAssist failed: Status: {}",
                    email,
                    res.status()
                ));
            }
        },
        Err(e) => {
            crate::modules::logger::log_error(&format!(
                "❌ [{}] loadCodeAssist network error: {}",
                email, e
            ));
        },
    }

    (None, None)
}

/// Unified entry point for querying account quota
pub async fn fetch_quota(
    access_token: &str,
    email: &str,
) -> crate::error::AppResult<(QuotaData, Option<String>)> {
    fetch_quota_inner(access_token, email, None).await
}

/// Unified entry point for querying account quota through a proxy.
pub async fn fetch_quota_proxied(
    access_token: &str,
    email: &str,
    proxy_url: Option<&str>,
) -> crate::error::AppResult<(QuotaData, Option<String>)> {
    fetch_quota_inner(access_token, email, proxy_url).await
}

/// Query account quota logic
pub async fn fetch_quota_inner(
    access_token: &str,
    email: &str,
    proxy_url: Option<&str>,
) -> crate::error::AppResult<(QuotaData, Option<String>)> {
    use crate::error::AppError;
    // crate::modules::logger::log_info(&format!("[{}] Starting external quota query...", email));

    // 1. Get Project ID and subscription type
    let client = create_client_proxied(proxy_url)?;
    let (project_id, subscription_tier) = fetch_project_id(&client, access_token, email).await;

    let final_project_id = project_id.as_deref().unwrap_or("bamboo-precept-lgxtn");

    let payload = json!({
        "project": final_project_id
    });

    let url = QUOTA_API_URL;
    let max_retries = 3;
    let mut last_error: Option<AppError> = None;

    for attempt in 1..=max_retries {
        match client
            .post(url)
            .bearer_auth(access_token)
            .header("User-Agent", &quota_user_agent())
            .json(&payload)
            .send()
            .await
        {
            Ok(response) => {
                // Convert HTTP error status to AppError
                if response.error_for_status_ref().is_err() {
                    let status = response.status();

                    // ✅ Special handling for 403 Forbidden - return directly, no retry
                    if status == reqwest::StatusCode::FORBIDDEN {
                        crate::modules::logger::log_warn(
                            "Account has no permission (403 Forbidden), marking as forbidden status",
                        );
                        let mut q = QuotaData::new();
                        q.is_forbidden = true;
                        q.subscription_tier = subscription_tier.clone();
                        return Ok((q, project_id.clone()));
                    }

                    // Other errors continue retry logic
                    let text = response.text().await.unwrap_or_default();
                    if attempt < max_retries {
                        crate::modules::logger::log_warn(&format!(
                            "API error: {} - {} (attempt {}/{})",
                            status, text, attempt, max_retries
                        ));
                        last_error = Some(AppError::Unknown(format!("HTTP {} - {}", status, text)));
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    } else {
                        return Err(AppError::Unknown(format!("API error: {} - {}", status, text)));
                    }
                }

                let quota_response: QuotaResponse =
                    response.json().await.map_err(AppError::Network)?;

                let mut quota_data = QuotaData::new();

                // Use debug level for detailed info to avoid console noise
                tracing::debug!("Quota API returned {} models", quota_response.models.len());

                for (name, info) in quota_response.models {
                    if let Some(quota_info) = info.quota_info {
                        let percentage =
                            quota_info.remaining_fraction.map(|f| (f * 100.0) as i32).unwrap_or(0);

                        let reset_time = quota_info.reset_time.unwrap_or_default();

                        // Only save models we care about
                        if antigravity_types::ModelFamily::from_model_name(&name)
                            != antigravity_types::ModelFamily::Unknown
                        {
                            quota_data.add_model(name, percentage, reset_time);
                        }
                    }
                }

                // Set subscription type
                quota_data.subscription_tier = subscription_tier.clone();

                return Ok((quota_data, project_id.clone()));
            },
            Err(e) => {
                crate::modules::logger::log_warn(&format!(
                    "Request failed: {} (attempt {}/{})",
                    e, attempt, max_retries
                ));
                last_error = Some(AppError::Network(e));
                if attempt < max_retries {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            },
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::Unknown("Quota query failed".to_string())))
}
