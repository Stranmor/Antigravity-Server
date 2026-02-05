//! Phone verification marking.

use crate::modules::logger;

use super::index::load_account_index;
use super::storage::{load_account, save_account};

pub async fn mark_needs_verification_by_email(email: &str) -> Result<(), String> {
    let index = load_account_index().map_err(|e| e.clone())?;
    let account_id = index
        .accounts
        .iter()
        .find(|acc| acc.email == email)
        .map(|acc| acc.id.clone())
        .ok_or_else(|| format!("Account not found: {}", email))?;

    tokio::task::spawn_blocking(move || {
        let mut account = load_account(&account_id)?;
        if !account.proxy_disabled
            || account.proxy_disabled_reason.as_deref() != Some("phone_verification_required")
        {
            account.proxy_disabled = true;
            account.proxy_disabled_reason = Some("phone_verification_required".to_string());
            save_account(&account)?;
            logger::log_warn(&format!(
                "Account {} marked as needing phone verification",
                account.email
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?
}
