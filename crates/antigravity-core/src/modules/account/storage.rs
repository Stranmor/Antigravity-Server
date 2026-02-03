//! Account file storage operations.

use std::fs;

use crate::models::Account;
use crate::modules::logger;

use super::index::{load_account_index, save_account_index};
use super::paths::get_accounts_dir;

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

/// Save a single account atomically.
pub fn save_account(account: &Account) -> Result<(), String> {
    let accounts_dir = get_accounts_dir()?;
    let account_path = accounts_dir.join(format!("{}.json", account.id));
    let temp_path = accounts_dir.join(format!("{}.json.tmp", account.id));

    let content = serde_json::to_string_pretty(account)
        .map_err(|e| format!("Failed to serialize account data: {}", e))?;

    if let Err(e) = fs::write(&temp_path, content) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("Failed to write temp account file: {}", e));
    }

    fs::rename(&temp_path, &account_path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!("Failed to replace account file: {}", e)
    })
}

/// List all accounts with auto-repair for missing/corrupted entries.
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
                if e.contains("not found")
                    || e.contains("No such file")
                    || e.contains("Failed to parse")
                {
                    invalid_ids.push(summary.id.clone());
                }
            }
        }
    }

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
