//! Account switching with token injection.

use std::fs;

use crate::modules::logger;

use super::async_wrappers::{load_account_async, save_account_async};
use super::index::{load_account_index, save_account_index, ACCOUNT_INDEX_LOCK};

pub async fn switch_account(account_id: &str) -> Result<(), String> {
    use crate::modules::{oauth, process, vscode};

    let index = {
        let _lock = ACCOUNT_INDEX_LOCK.lock().map_err(|e| format!("Lock error: {}", e))?;
        load_account_index()?
    };

    if !index.accounts.iter().any(|s| s.id == account_id) {
        return Err(format!("Account not found: {}", account_id));
    }

    let account_id_owned = account_id.to_string();
    let mut account = load_account_async(account_id_owned.clone()).await?;
    logger::log_info(&format!("Switching to account: {} (ID: {})", account.email, account.id));

    let fresh_token = oauth::ensure_fresh_token(&account.token)
        .await
        .map_err(|e| format!("Token refresh failed: {}", e))?;

    if fresh_token.access_token != account.token.access_token {
        account.token = fresh_token.clone();
        save_account_async(account.clone()).await?;
    }

    let is_running = tokio::task::spawn_blocking(process::is_antigravity_running)
        .await
        .map_err(|e| format!("Task join error: {}", e))?;
    if is_running {
        tokio::task::spawn_blocking(|| process::close_antigravity(20))
            .await
            .map_err(|e| format!("Task join error: {}", e))??;
    }

    let db_path = tokio::task::spawn_blocking(vscode::get_vscode_db_path)
        .await
        .map_err(|e| format!("Task join error: {}", e))??;

    if db_path.exists() {
        let backup_path = db_path.with_extension("vscdb.backup");
        let db_path_clone = db_path.clone();
        tokio::task::spawn_blocking(move || fs::copy(&db_path_clone, &backup_path))
            .await
            .map_err(|e| format!("Task join error: {}", e))?
            .map_err(|e| format!("Backup DB failed: {}", e))?;
    } else {
        logger::log_info("DB not found, skipping backup");
    }

    logger::log_info("Injecting token to DB...");
    let access_token = account.token.access_token.clone();
    let refresh_token = account.token.refresh_token.clone();
    let expiry = account.token.expiry_timestamp;
    tokio::task::spawn_blocking(move || {
        vscode::inject_token(&db_path, &access_token, &refresh_token, expiry)
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))??;

    {
        let account_id_str = account_id_owned.clone();
        tokio::task::spawn_blocking(move || {
            let _lock = ACCOUNT_INDEX_LOCK.lock().map_err(|e| format!("Lock error: {}", e))?;
            let mut index = load_account_index()?;
            index.current_account_id = Some(account_id_str);
            save_account_index(&index)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))??;
    }

    account.update_last_used();
    save_account_async(account.clone()).await?;

    tokio::task::spawn_blocking(process::start_antigravity)
        .await
        .map_err(|e| format!("Task join error: {}", e))??;
    logger::log_info(&format!("Account switch complete: {}", account.email));

    Ok(())
}
