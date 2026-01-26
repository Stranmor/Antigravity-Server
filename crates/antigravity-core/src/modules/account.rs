//! Account management module.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use once_cell::sync::Lazy;
use uuid::Uuid;

use crate::models::{Account, AccountIndex, AccountSummary, QuotaData, TokenData};
use crate::modules::logger;

/// Global lock for account index operations to prevent concurrent corruption.
static ACCOUNT_INDEX_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// Directory and file names
const DATA_DIR: &str = ".antigravity_tools";
const ACCOUNTS_INDEX: &str = "accounts.json";
const ACCOUNTS_DIR: &str = "accounts";

/// Get the data directory path.
pub fn get_data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("Cannot get home directory")?;
    let data_dir = home.join(DATA_DIR);

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data directory: {}", e))?;
    }

    Ok(data_dir)
}

/// Get the accounts directory path.
pub fn get_accounts_dir() -> Result<PathBuf, String> {
    let data_dir = get_data_dir()?;
    let accounts_dir = data_dir.join(ACCOUNTS_DIR);

    if !accounts_dir.exists() {
        fs::create_dir_all(&accounts_dir)
            .map_err(|e| format!("Failed to create accounts directory: {}", e))?;
    }

    Ok(accounts_dir)
}

/// Load the account index file.
pub fn load_account_index() -> Result<AccountIndex, String> {
    let data_dir = get_data_dir()?;
    let index_path = data_dir.join(ACCOUNTS_INDEX);

    if !index_path.exists() {
        logger::log_warn("Account index file does not exist");
        return Ok(AccountIndex::new());
    }

    let content = fs::read_to_string(&index_path)
        .map_err(|e| format!("Failed to read account index: {}", e))?;

    let index: AccountIndex = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse account index: {}", e))?;

    logger::log_info(&format!(
        "Loaded index with {} accounts",
        index.accounts.len()
    ));
    Ok(index)
}

/// Save the account index file atomically.
pub fn save_account_index(index: &AccountIndex) -> Result<(), String> {
    let data_dir = get_data_dir()?;
    let index_path = data_dir.join(ACCOUNTS_INDEX);
    let temp_path = data_dir.join(format!("{}.tmp", ACCOUNTS_INDEX));

    let content = serde_json::to_string_pretty(index)
        .map_err(|e| format!("Failed to serialize account index: {}", e))?;

    // Write to temp file first
    fs::write(&temp_path, content)
        .map_err(|e| format!("Failed to write temp index file: {}", e))?;

    // Atomic rename
    fs::rename(temp_path, index_path).map_err(|e| format!("Failed to replace index file: {}", e))
}

/// Load a single account by ID.
pub fn load_account(account_id: &str) -> Result<Account, String> {
    let accounts_dir = get_accounts_dir()?;
    let account_path = accounts_dir.join(format!("{}.json", account_id));

    if !account_path.exists() {
        return Err(format!("Account not found: {}", account_id));
    }

    let content = fs::read_to_string(&account_path)
        .map_err(|e| format!("Failed to read account data: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse account data: {}", e))
}

/// Save a single account.
pub fn save_account(account: &Account) -> Result<(), String> {
    let accounts_dir = get_accounts_dir()?;
    let account_path = accounts_dir.join(format!("{}.json", account.id));

    let content = serde_json::to_string_pretty(account)
        .map_err(|e| format!("Failed to serialize account data: {}", e))?;

    fs::write(&account_path, content).map_err(|e| format!("Failed to save account data: {}", e))
}

/// List all accounts.
pub fn list_accounts() -> Result<Vec<Account>, String> {
    logger::log_info("Listing accounts...");
    let mut index = load_account_index()?;
    let mut accounts = Vec::new();
    let mut invalid_ids = Vec::new();

    for summary in &index.accounts {
        match load_account(&summary.id) {
            Ok(account) => accounts.push(account),
            Err(e) => {
                logger::log_error(&format!("Failed to load account {}: {}", summary.id, e));
                // Auto-repair: remove missing or corrupted accounts
                if e.contains("not found")
                    || e.contains("No such file")
                    || e.contains("Failed to parse")
                {
                    invalid_ids.push(summary.id.clone());
                }
            }
        }
    }

    // Auto-repair index by removing invalid IDs
    if !invalid_ids.is_empty() {
        logger::log_warn(&format!(
            "Found {} invalid account indices, cleaning up...",
            invalid_ids.len()
        ));

        index.accounts.retain(|s| !invalid_ids.contains(&s.id));

        if let Some(current_id) = &index.current_account_id {
            if invalid_ids.contains(current_id) {
                index.current_account_id = index.accounts.first().map(|s| s.id.clone());
            }
        }

        if let Err(e) = save_account_index(&index) {
            logger::log_error(&format!("Failed to save cleaned index: {}", e));
        } else {
            logger::log_info("Index cleanup complete");
        }
    }

    Ok(accounts)
}

/// Add a new account.
pub fn add_account(
    email: String,
    name: Option<String>,
    token: TokenData,
) -> Result<Account, String> {
    let _lock = ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let mut index = load_account_index()?;

    // Check if already exists
    if index.accounts.iter().any(|s| s.email == email) {
        return Err(format!("Account already exists: {}", email));
    }

    // Create new account
    let account_id = Uuid::new_v4().to_string();
    let mut account = Account::new(account_id.clone(), email.clone(), token);
    account.name = name.clone();

    // Save account data
    save_account(&account)?;

    // Update index
    index.accounts.push(AccountSummary {
        id: account_id.clone(),
        email: email.clone(),
        name: name.clone(),
        created_at: account.created_at,
        last_used: account.last_used,
    });

    // Set as current if first account
    if index.current_account_id.is_none() {
        index.current_account_id = Some(account_id);
    }

    save_account_index(&index)?;

    Ok(account)
}

/// Add or update an account (upsert).
pub fn upsert_account(
    email: String,
    name: Option<String>,
    token: TokenData,
) -> Result<Account, String> {
    let _lock = ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let mut index = load_account_index()?;

    // Find existing account ID
    let existing_account_id = index
        .accounts
        .iter()
        .find(|s| s.email == email)
        .map(|s| s.id.clone());

    if let Some(account_id) = existing_account_id {
        // Update existing account
        match load_account(&account_id) {
            Ok(mut account) => {
                let old_access_token = account.token.access_token.clone();
                let old_refresh_token = account.token.refresh_token.clone();
                account.token = token;
                account.name = name.clone();

                // Re-enable if credentials were updated
                if account.disabled
                    && (account.token.refresh_token != old_refresh_token
                        || account.token.access_token != old_access_token)
                {
                    account.disabled = false;
                    account.disabled_reason = None;
                    account.disabled_at = None;
                }
                account.update_last_used();
                save_account(&account)?;

                // Update name in index
                if let Some(idx_summary) = index.accounts.iter_mut().find(|s| s.id == account_id) {
                    idx_summary.name = name;
                    save_account_index(&index)?;
                }

                return Ok(account);
            }
            Err(e) => {
                logger::log_warn(&format!(
                    "Account {} file missing ({}), recreating...",
                    account_id, e
                ));
                let mut account = Account::new(account_id.clone(), email.clone(), token);
                account.name = name.clone();
                save_account(&account)?;

                if let Some(idx_summary) = index.accounts.iter_mut().find(|s| s.id == account_id) {
                    idx_summary.name = name;
                    save_account_index(&index)?;
                }

                return Ok(account);
            }
        }
    }

    // Release lock and add new account
    drop(_lock);
    add_account(email, name, token)
}

/// Delete an account by ID.
pub fn delete_account(account_id: &str) -> Result<(), String> {
    let _lock = ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let mut index = load_account_index()?;

    let original_len = index.accounts.len();
    index.accounts.retain(|s| s.id != account_id);

    if index.accounts.len() == original_len {
        return Err(format!("Account not found: {}", account_id));
    }

    // Update current account if necessary
    if index.current_account_id.as_deref() == Some(account_id) {
        index.current_account_id = index.accounts.first().map(|s| s.id.clone());
    }

    save_account_index(&index)?;

    // Delete account file
    let accounts_dir = get_accounts_dir()?;
    let account_path = accounts_dir.join(format!("{}.json", account_id));

    if account_path.exists() {
        fs::remove_file(&account_path)
            .map_err(|e| format!("Failed to delete account file: {}", e))?;
    }

    Ok(())
}

/// Batch delete accounts.
pub fn delete_accounts(account_ids: &[String]) -> Result<(), String> {
    let _lock = ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let mut index = load_account_index()?;
    let accounts_dir = get_accounts_dir()?;

    for account_id in account_ids {
        index.accounts.retain(|s| &s.id != account_id);

        if index.current_account_id.as_deref() == Some(account_id) {
            index.current_account_id = None;
        }

        let account_path = accounts_dir.join(format!("{}.json", account_id));
        if account_path.exists() {
            let _ = fs::remove_file(&account_path);
        }
    }

    if index.current_account_id.is_none() {
        index.current_account_id = index.accounts.first().map(|s| s.id.clone());
    }

    save_account_index(&index)
}

/// Reorder accounts based on the provided ID order.
pub fn reorder_accounts(account_ids: &[String]) -> Result<(), String> {
    let _lock = ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let mut index = load_account_index()?;

    let id_to_summary: std::collections::HashMap<_, _> = index
        .accounts
        .iter()
        .map(|s| (s.id.clone(), s.clone()))
        .collect();

    let mut new_accounts = Vec::new();
    for id in account_ids {
        if let Some(summary) = id_to_summary.get(id) {
            new_accounts.push(summary.clone());
        }
    }

    // Add accounts not in the new order
    for summary in &index.accounts {
        if !account_ids.contains(&summary.id) {
            new_accounts.push(summary.clone());
        }
    }

    index.accounts = new_accounts;

    logger::log_info(&format!(
        "Account order updated, {} accounts",
        index.accounts.len()
    ));

    save_account_index(&index)
}

/// Get the current account ID.
pub fn get_current_account_id() -> Result<Option<String>, String> {
    let index = load_account_index()?;
    Ok(index.current_account_id)
}

/// Get the current active account.
pub fn get_current_account() -> Result<Option<Account>, String> {
    if let Some(id) = get_current_account_id()? {
        Ok(Some(load_account(&id)?))
    } else {
        Ok(None)
    }
}

/// Set the current account ID.
pub fn set_current_account_id(account_id: &str) -> Result<(), String> {
    let _lock = ACCOUNT_INDEX_LOCK
        .lock()
        .map_err(|e| format!("Lock error: {}", e))?;
    let mut index = load_account_index()?;
    index.current_account_id = Some(account_id.to_string());
    save_account_index(&index)
}

/// Update account quota with quota protection logic.
///
/// When quota protection is enabled in config:
/// - If a monitored model's percentage <= threshold, the model is added to protected_models
/// - If a monitored model's percentage > threshold and was protected, it's removed from protected_models
/// - This allows per-model protection instead of disabling the entire account
pub fn update_account_quota(account_id: &str, quota: QuotaData) -> Result<(), String> {
    use crate::modules::config::load_config;
    use crate::proxy::common::model_mapping::normalize_to_standard_id;

    let mut account = load_account(account_id)?;
    account.update_quota(quota.clone());

    // --- Quota protection logic ---
    let config_result = load_config();
    if let Ok(config) = config_result {
        if config.quota_protection.enabled {
            let threshold = config.quota_protection.threshold_percentage as i32;
            logger::log_info(&format!(
                "[Quota Protection] Processing {} models for {}, threshold={}%",
                quota.models.len(),
                account.email,
                threshold
            ));

            for model in &quota.models {
                // Normalize model name to standard ID (e.g., "gemini-2.5-flash" -> "gemini-3-flash")
                let standard_id = match normalize_to_standard_id(&model.name) {
                    Some(id) => id,
                    None => continue, // Not one of the 3 protected models, skip
                };

                // Only monitor models that user has checked in config
                if !config
                    .quota_protection
                    .monitored_models
                    .contains(&standard_id)
                {
                    continue;
                }

                if model.percentage <= threshold {
                    // Trigger model-level protection
                    if !account.is_model_protected(&standard_id) {
                        logger::log_info(&format!(
                            "[Quota] Protecting model: {} ({} [{}] at {}% <= threshold {}%)",
                            account.email, standard_id, model.name, model.percentage, threshold
                        ));
                        account.protect_model(&standard_id);
                    }
                } else if config.quota_protection.auto_restore {
                    // Auto-restore if above threshold
                    if account.is_model_protected(&standard_id) {
                        logger::log_info(&format!(
                            "[Quota] Restoring model: {} ({} [{}] quota recovered to {}%)",
                            account.email, standard_id, model.name, model.percentage
                        ));
                        account.unprotect_model(&standard_id);
                    }
                }
            }

            // [Compatibility] If account was previously disabled at account-level due to quota_protection,
            // migrate to model-level protection
            if account.proxy_disabled
                && account
                    .proxy_disabled_reason
                    .as_ref()
                    .is_some_and(|r| r == "quota_protection")
            {
                logger::log_info(&format!(
                    "[Quota] Migrating account {} from account-level to model-level protection",
                    account.email
                ));
                account.enable_for_proxy();
            }
        }
    }
    // --- End quota protection logic ---

    save_account(&account)
}

/// Switch current account.
/// This involves refreshing token, stopping/starting process, and injecting token to VSCode DB.
pub async fn switch_account(account_id: &str) -> Result<(), String> {
    use crate::modules::{oauth, process, vscode};

    let index = {
        let _lock = ACCOUNT_INDEX_LOCK
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        load_account_index()?
    };

    // 1. Verify account exists
    if !index.accounts.iter().any(|s| s.id == account_id) {
        return Err(format!("Account not found: {}", account_id));
    }

    let mut account = load_account(account_id)?;
    logger::log_info(&format!(
        "Switching to account: {} (ID: {})",
        account.email, account.id
    ));

    // 2. Ensure token is fresh
    let fresh_token = oauth::ensure_fresh_token(&account.token)
        .await
        .map_err(|e| format!("Token refresh failed: {}", e))?;

    if fresh_token.access_token != account.token.access_token {
        account.token = fresh_token.clone();
        save_account(&account)?;
    }

    // 3. Close Antigravity (timeout 20s)
    if process::is_antigravity_running() {
        process::close_antigravity(20)?;
    }

    // 4. Backup and Inject DB
    let db_path = vscode::get_vscode_db_path()?;
    if db_path.exists() {
        let backup_path = db_path.with_extension("vscdb.backup");
        fs::copy(&db_path, &backup_path).map_err(|e| format!("Backup DB failed: {}", e))?;
    } else {
        logger::log_info("DB not found, skipping backup");
    }

    logger::log_info("Injecting token to DB...");
    vscode::inject_token(
        &db_path,
        &account.token.access_token,
        &account.token.refresh_token,
        account.token.expiry_timestamp,
    )?;

    // 5. Update internal state
    {
        let _lock = ACCOUNT_INDEX_LOCK
            .lock()
            .map_err(|e| format!("Lock error: {}", e))?;
        let mut index = load_account_index()?;
        index.current_account_id = Some(account_id.to_string());
        save_account_index(&index)?;
    }

    account.update_last_used();
    save_account(&account)?;

    // 6. Restart Antigravity
    process::start_antigravity()?;
    logger::log_info(&format!("Account switch complete: {}", account.email));

    Ok(())
}

/// Fetch quota with retry logic.
pub async fn fetch_quota_with_retry(account: &mut Account) -> crate::error::AppResult<QuotaData> {
    use crate::error::AppError;
    use crate::modules::{oauth, quota};
    use reqwest::StatusCode;

    // 1. Time-based check
    let token = match oauth::ensure_fresh_token(&account.token).await {
        Ok(t) => t,
        Err(e) => {
            if e.contains("invalid_grant") {
                logger::log_error(&format!(
                    "Disabling account {} due to invalid_grant during token refresh (quota check)",
                    account.email
                ));
                account.disabled = true;
                account.disabled_at = Some(chrono::Utc::now().timestamp());
                account.disabled_reason = Some(format!("invalid_grant: {}", e));
                let _ = save_account(account);
            }
            return Err(AppError::OAuth(e));
        }
    };

    if token.access_token != account.token.access_token {
        logger::log_info(&format!("Time-based token refresh: {}", account.email));
        account.token = token.clone();

        let name = if account.name.is_none()
            || account.name.as_ref().is_some_and(|n| n.trim().is_empty())
        {
            match oauth::get_user_info(&token.access_token).await {
                Ok(user_info) => user_info.get_display_name(),
                Err(_) => None,
            }
        } else {
            account.name.clone()
        };

        account.name = name.clone();
        upsert_account(account.email.clone(), name, token.clone()).map_err(AppError::Account)?;
    }

    // 0. Fill missing username
    if account.name.is_none() || account.name.as_ref().is_some_and(|n| n.trim().is_empty()) {
        logger::log_info(&format!(
            "Account {} missing name, fetching...",
            account.email
        ));
        match oauth::get_user_info(&account.token.access_token).await {
            Ok(user_info) => {
                let display_name = user_info.get_display_name();
                logger::log_info(&format!("Got name: {:?}", display_name));
                account.name = display_name.clone();
                if let Err(e) =
                    upsert_account(account.email.clone(), display_name, account.token.clone())
                {
                    logger::log_warn(&format!("Failed to save name: {}", e));
                }
            }
            Err(e) => {
                logger::log_warn(&format!("Failed to get name: {}", e));
            }
        }
    }

    // 2. Fetch quota
    let result = quota::fetch_quota(&account.token.access_token, &account.email).await;

    // Update quota and project_id if successful
    if let Ok((ref quota_data, ref project_id)) = result {
        // Always update quota data
        account.update_quota(quota_data.clone());

        // Update project_id if changed
        if project_id.is_some() && *project_id != account.token.project_id {
            logger::log_info(&format!(
                "Project ID updated ({}), saving...",
                account.email
            ));
            account.token.project_id = project_id.clone();
        }

        // Save account with updated quota
        if let Err(e) = save_account(account) {
            logger::log_warn(&format!(
                "Failed to save quota for {}: {}",
                account.email, e
            ));
        }
    }

    // 3. Handle 401
    if let Err(AppError::Network(ref e)) = result {
        if let Some(status) = e.status() {
            if status == StatusCode::UNAUTHORIZED {
                logger::log_warn(&format!(
                    "401 Unauthorized for {}, forcing refresh...",
                    account.email
                ));

                let token_res =
                    match oauth::refresh_access_token(&account.token.refresh_token).await {
                        Ok(t) => t,
                        Err(e) => {
                            if e.contains("invalid_grant") {
                                logger::log_error(&format!(
                                "Disabling account {} due to invalid_grant during forced refresh",
                                account.email
                            ));
                                account.disabled = true;
                                account.disabled_at = Some(chrono::Utc::now().timestamp());
                                account.disabled_reason = Some(format!("invalid_grant: {}", e));
                                let _ = save_account(account);
                            }
                            return Err(AppError::OAuth(e));
                        }
                    };

                let new_token = TokenData::new(
                    token_res.access_token.clone(),
                    account.token.refresh_token.clone(),
                    token_res.expires_in,
                    account.token.email.clone(),
                    account.token.project_id.clone(),
                    None,
                );

                let name = if account.name.is_none()
                    || account.name.as_ref().is_some_and(|n| n.trim().is_empty())
                {
                    match oauth::get_user_info(&token_res.access_token).await {
                        Ok(user_info) => user_info.get_display_name(),
                        Err(_) => None,
                    }
                } else {
                    account.name.clone()
                };

                account.token = new_token.clone();
                account.name = name.clone();
                upsert_account(account.email.clone(), name, new_token.clone())
                    .map_err(AppError::Account)?;

                // Retry
                let retry_result =
                    quota::fetch_quota(&new_token.access_token, &account.email).await;

                if let Ok((ref quota_data, ref project_id)) = retry_result {
                    account.update_quota(quota_data.clone());

                    if project_id.is_some() && *project_id != account.token.project_id {
                        logger::log_info(&format!(
                            "Project ID updated after retry ({}), saving...",
                            account.email
                        ));
                        account.token.project_id = project_id.clone();
                    }

                    let _ = save_account(account);
                }

                if let Err(AppError::Network(ref e)) = retry_result {
                    if let Some(s) = e.status() {
                        if s == StatusCode::FORBIDDEN {
                            let mut q = QuotaData::new();
                            q.is_forbidden = true;
                            return Ok(q);
                        }
                    }
                }
                return retry_result.map(|(q, _)| q);
            }
        }
    }

    result.map(|(q, _)| q)
}
