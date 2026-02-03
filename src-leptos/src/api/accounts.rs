//! Account-related API calls

use super::{api_get, api_post};
use crate::api_models::*;

#[derive(serde::Deserialize, Clone, Debug)]
pub struct ApiAccount {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub disabled: bool,
    #[serde(default)]
    pub proxy_disabled: bool,
    pub is_current: bool,
    pub gemini_quota: Option<i32>,
    pub claude_quota: Option<i32>,
    pub image_quota: Option<i32>,
    pub subscription_tier: Option<String>,
    pub quota: Option<antigravity_types::models::QuotaData>,
}

impl ApiAccount {
    pub fn into_account(self) -> Account {
        use antigravity_types::models::TokenData;
        use std::collections::HashSet;

        Account {
            id: self.id,
            email: self.email,
            name: self.name,
            token: TokenData::new(String::new(), String::new(), 0, None, None, None),
            quota: self.quota,
            disabled: self.disabled,
            disabled_reason: None,
            disabled_at: None,
            proxy_disabled: self.proxy_disabled,
            proxy_disabled_reason: None,
            proxy_disabled_at: None,
            protected_models: HashSet::new(),
            created_at: 0,
            last_used: 0,
        }
    }
}

pub async fn list_accounts() -> Result<Vec<Account>, String> {
    let api_accounts: Vec<ApiAccount> = api_get("/accounts").await?;
    Ok(api_accounts.into_iter().map(|a| a.into_account()).collect())
}

pub async fn get_current_account() -> Result<Option<Account>, String> {
    let api_account: Option<ApiAccount> = api_get("/accounts/current").await?;
    Ok(api_account.map(|a| a.into_account()))
}

pub async fn switch_account(account_id: &str) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Req {
        account_id: String,
    }

    let _: bool = api_post(
        "/accounts/switch",
        &Req {
            account_id: account_id.to_string(),
        },
    )
    .await?;
    Ok(())
}

pub async fn delete_account(account_id: &str) -> Result<(), String> {
    let _: bool = api_post(
        "/accounts/delete",
        &serde_json::json!({
            "account_id": account_id
        }),
    )
    .await?;
    Ok(())
}

pub async fn delete_accounts(account_ids: &[String]) -> Result<(), String> {
    let _: bool = api_post(
        "/accounts/delete-batch",
        &serde_json::json!({
            "account_ids": account_ids
        }),
    )
    .await?;
    Ok(())
}

pub async fn toggle_proxy_status(
    account_id: &str,
    enable: bool,
    reason: Option<&str>,
) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct ToggleProxyResponse {
        #[allow(dead_code)]
        success: bool,
        #[allow(dead_code)]
        account_id: String,
        #[allow(dead_code)]
        proxy_disabled: bool,
    }

    let _: ToggleProxyResponse = api_post(
        "/accounts/toggle-proxy",
        &serde_json::json!({
            "account_id": account_id,
            "enable": enable,
            "reason": reason
        }),
    )
    .await?;
    Ok(())
}

pub async fn warmup_account(account_id: &str) -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct WarmupResponse {
        message: String,
    }
    let response: WarmupResponse = api_post(
        "/accounts/warmup",
        &serde_json::json!({
            "account_id": account_id
        }),
    )
    .await?;
    Ok(response.message)
}

pub async fn warmup_all_accounts() -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct WarmupResponse {
        message: String,
    }
    let response: WarmupResponse = api_post("/accounts/warmup-all", &serde_json::json!({})).await?;
    Ok(response.message)
}

// ========== OAuth ==========

#[derive(serde::Deserialize)]
struct OAuthLoginResponse {
    url: String,
    #[allow(dead_code)]
    message: String,
}

pub async fn start_oauth_login() -> Result<Account, String> {
    let response: OAuthLoginResponse = api_post("/oauth/login", &serde_json::json!({})).await?;

    let window = web_sys::window().ok_or("No window")?;
    window
        .open_with_url_and_target(&response.url, "_blank")
        .map_err(|e| format!("Failed to open browser: {:?}", e))?;

    Err("OAuth flow started - check browser and refresh accounts list after login".to_string())
}

#[derive(serde::Deserialize)]
struct OAuthUrlResponse {
    url: String,
    #[allow(dead_code)]
    redirect_uri: String,
}

pub async fn prepare_oauth_url() -> Result<String, String> {
    let response: OAuthUrlResponse = api_get("/oauth/url").await?;
    Ok(response.url)
}

pub async fn cancel_oauth_login() -> Result<(), String> {
    Ok(())
}

// ========== Quota ==========

#[derive(serde::Deserialize)]
struct QuotaResponse {
    #[allow(dead_code)]
    account_id: String,
    quota: Option<QuotaData>,
}

pub async fn fetch_account_quota(account_id: &str) -> Result<QuotaData, String> {
    let response: QuotaResponse = api_post(
        "/accounts/refresh-quota",
        &serde_json::json!({
            "account_id": account_id
        }),
    )
    .await?;
    response
        .quota
        .ok_or_else(|| "No quota data returned".to_string())
}

pub async fn refresh_all_quotas() -> Result<RefreshStats, String> {
    api_post("/accounts/refresh-all-quotas", &serde_json::json!({})).await
}

// ========== Token-based Add ==========

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct AddByTokenResponse {
    success_count: usize,
    fail_count: usize,
    accounts: Vec<Account>,
}

pub async fn add_accounts_by_token(refresh_tokens: Vec<String>) -> Result<(usize, usize), String> {
    let response: AddByTokenResponse = api_post(
        "/accounts/add-by-token",
        &serde_json::json!({
            "refresh_tokens": refresh_tokens
        }),
    )
    .await?;
    Ok((response.success_count, response.fail_count))
}

// ========== Import ==========

pub async fn sync_account_from_db() -> Result<Option<Account>, String> {
    Ok(None)
}

pub async fn import_custom_db(_path: &str) -> Result<Account, String> {
    Err("Import from file not available in browser mode".to_string())
}
