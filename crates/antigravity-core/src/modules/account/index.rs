//! Account index management with atomic operations.

use std::fs;
use std::sync::Mutex;

use once_cell::sync::Lazy;

use crate::models::AccountIndex;
use crate::modules::logger;

use super::paths::{get_data_dir, ACCOUNTS_INDEX};

/// Global lock for account index operations to prevent concurrent corruption.
pub static ACCOUNT_INDEX_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

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

    fs::write(&temp_path, content)
        .map_err(|e| format!("Failed to write temp index file: {}", e))?;

    fs::rename(temp_path, index_path).map_err(|e| format!("Failed to replace index file: {}", e))
}
