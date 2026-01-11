//! Account management module.
//!
//! This module handles account storage, indexing, and CRUD operations.
//! It uses JSON files for persistence.

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

/// Update account quota.
pub fn update_account_quota(account_id: &str, quota: QuotaData) -> Result<(), String> {
    let mut account = load_account(account_id)?;
    account.update_quota(quota);
    save_account(&account)
}
