//! Async wrappers for blocking account operations.

use crate::models::{Account, QuotaData, TokenData};

use super::crud::upsert_account;
use super::quota::update_account_quota;
use super::storage::{load_account, save_account};

pub async fn save_account_async(account: Account) -> Result<(), String> {
    tokio::task::spawn_blocking(move || save_account(&account))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

pub async fn load_account_async(account_id: String) -> Result<Account, String> {
    tokio::task::spawn_blocking(move || load_account(&account_id))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

pub async fn update_account_quota_async(
    account_id: String,
    quota: QuotaData,
) -> Result<Account, String> {
    tokio::task::spawn_blocking(move || update_account_quota(&account_id, quota))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}

pub async fn upsert_account_async(
    email: String,
    name: Option<String>,
    token: TokenData,
) -> Result<Account, String> {
    tokio::task::spawn_blocking(move || upsert_account(email, name, token))
        .await
        .map_err(|e| format!("Task join error: {}", e))?
}
