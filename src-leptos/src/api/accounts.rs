//! Account-related API calls

use antigravity_types::models::QuotaData;

use super::{api_get, api_post};
use crate::api_models::*;

#[derive(serde::Deserialize, Clone, Debug)]
pub(crate) struct ApiAccount {
    pub(crate) id: String,
    pub(crate) email: String,
    pub(crate) name: Option<String>,
    pub(crate) disabled: bool,
    #[serde(default)]
    pub(crate) proxy_disabled: bool,
    #[allow(dead_code, reason = "deserialized from API response")]
    is_current: bool,
    #[allow(dead_code, reason = "deserialized from API response")]
    gemini_quota: Option<i32>,
    #[allow(dead_code, reason = "deserialized from API response")]
    claude_quota: Option<i32>,
    #[allow(dead_code, reason = "deserialized from API response")]
    image_quota: Option<i32>,
    #[allow(dead_code, reason = "deserialized from API response")]
    subscription_tier: Option<String>,
    pub(crate) quota: Option<QuotaData>,
}

impl ApiAccount {
    pub(crate) fn into_account(self) -> Account {
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

pub(crate) async fn list_accounts() -> Result<Vec<Account>, String> {
    let api_accounts: Vec<ApiAccount> = api_get("/accounts").await?;
    Ok(api_accounts.into_iter().map(|a| a.into_account()).collect())
}

pub(crate) async fn get_current_account() -> Result<Option<Account>, String> {
    let api_account: Option<ApiAccount> = api_get("/accounts/current").await?;
    Ok(api_account.map(|a| a.into_account()))
}

pub(crate) async fn switch_account(account_id: &str) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Req {
        account_id: String,
    }

    let _: bool = api_post("/accounts/switch", &Req { account_id: account_id.to_string() }).await?;
    Ok(())
}

pub(crate) async fn delete_account(account_id: &str) -> Result<(), String> {
    let _: bool = api_post(
        "/accounts/delete",
        &serde_json::json!({
            "account_id": account_id
        }),
    )
    .await?;
    Ok(())
}

pub(crate) async fn delete_accounts(account_ids: &[String]) -> Result<(), String> {
    let _: bool = api_post(
        "/accounts/delete-batch",
        &serde_json::json!({
            "account_ids": account_ids
        }),
    )
    .await?;
    Ok(())
}

pub(crate) async fn toggle_proxy_status(
    account_id: &str,
    enable: bool,
    reason: Option<&str>,
) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct ToggleProxyResponse {
        #[allow(dead_code, reason = "deserialized from API response")]
        success: bool,
        #[allow(dead_code, reason = "deserialized from API response")]
        account_id: String,
        #[allow(dead_code, reason = "deserialized from API response")]
        proxy_disabled: bool,
    }

    drop(
        api_post::<_, ToggleProxyResponse>(
            "/accounts/toggle-proxy",
            &serde_json::json!({
                "account_id": account_id,
                "enable": enable,
                "reason": reason
            }),
        )
        .await?,
    );
    Ok(())
}

pub(crate) async fn warmup_account(account_id: &str) -> Result<String, String> {
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

pub(crate) async fn warmup_all_accounts() -> Result<String, String> {
    #[derive(serde::Deserialize)]
    struct WarmupResponse {
        message: String,
    }
    let response: WarmupResponse = api_post("/accounts/warmup-all", &serde_json::json!({})).await?;
    Ok(response.message)
}

#[derive(serde::Deserialize)]
struct OAuthUrlResponse {
    url: String,
    #[allow(dead_code, reason = "deserialized from API response")]
    redirect_uri: String,
}

pub(crate) async fn prepare_oauth_url() -> Result<String, String> {
    let response: OAuthUrlResponse = api_get("/oauth/url").await?;
    Ok(response.url)
}

#[derive(serde::Deserialize)]
struct QuotaResponse {
    #[allow(dead_code, reason = "deserialized from API response")]
    account_id: String,
    quota: Option<QuotaData>,
}

pub(crate) async fn fetch_account_quota(account_id: &str) -> Result<QuotaData, String> {
    let response: QuotaResponse = api_post(
        "/accounts/refresh-quota",
        &serde_json::json!({
            "account_id": account_id
        }),
    )
    .await?;
    response.quota.ok_or_else(|| "No quota data returned".to_string())
}

pub(crate) async fn refresh_all_quotas() -> Result<RefreshStats, String> {
    api_post("/accounts/refresh-all-quotas", &serde_json::json!({})).await
}

#[derive(serde::Deserialize)]
#[allow(dead_code, reason = "deserialized from API response")]
struct AddByTokenResponse {
    success_count: usize,
    fail_count: usize,
    accounts: Vec<Account>,
}

pub(crate) async fn add_accounts_by_token(
    refresh_tokens: Vec<String>,
) -> Result<(usize, usize), String> {
    let response: AddByTokenResponse = api_post(
        "/accounts/add-by-token",
        &serde_json::json!({
            "refresh_tokens": refresh_tokens
        }),
    )
    .await?;
    Ok((response.success_count, response.fail_count))
}

pub(crate) async fn sync_account_from_db() -> Result<Option<Account>, String> {
    Ok(None)
}
