//! Quota update with protection logic.

use crate::models::{Account, QuotaData};
use crate::modules::logger;

use super::index::ACCOUNT_INDEX_LOCK;
use super::storage::{load_account, save_account};

/// Update account quota with quota protection logic.
///
/// When quota protection is enabled in config:
/// - If a monitored model's percentage <= threshold, the model is added to protected_models
/// - If a monitored model's percentage > threshold and was protected, it's removed
pub fn update_account_quota(account_id: &str, quota: QuotaData) -> Result<Account, String> {
    use crate::modules::config::load_config;
    use crate::proxy::common::model_mapping::normalize_to_standard_id;

    let _lock = ACCOUNT_INDEX_LOCK.lock().map_err(|e| format!("Lock error: {}", e))?;

    let mut account = load_account(account_id)?;
    account.update_quota(quota.clone());

    let config_result = load_config();
    if let Ok(config) = config_result {
        if config.quota_protection.enabled {
            let threshold = i32::from(config.quota_protection.threshold_percentage);

            if quota.is_forbidden {
                logger::log_info(&format!(
                    "[Quota Protection] Account {} is forbidden, protecting all monitored models",
                    account.email
                ));
                for model_id in &config.quota_protection.monitored_models {
                    if !account.is_model_protected(model_id) {
                        account.protect_model(model_id);
                    }
                }
            } else {
                logger::log_info(&format!(
                    "[Quota Protection] Processing {} models for {}, threshold={}%",
                    quota.models.len(),
                    account.email,
                    threshold
                ));

                for model in &quota.models {
                    let standard_id = match normalize_to_standard_id(&model.name) {
                        Some(id) => id,
                        None => continue,
                    };

                    if !config.quota_protection.monitored_models.contains(&standard_id) {
                        continue;
                    }

                    if model.percentage <= threshold {
                        if !account.is_model_protected(&standard_id) {
                            logger::log_info(&format!(
                                "[Quota] Protecting model: {} ({} [{}] at {}% <= threshold {}%)",
                                account.email, standard_id, model.name, model.percentage, threshold
                            ));
                            account.protect_model(&standard_id);
                        }
                    } else if config.quota_protection.auto_restore
                        && account.is_model_protected(&standard_id)
                    {
                        logger::log_info(&format!(
                            "[Quota] Restoring model: {} ({} [{}] quota recovered to {}%)",
                            account.email, standard_id, model.name, model.percentage
                        ));
                        account.unprotect_model(&standard_id);
                    }
                }
            }

            if account.proxy_disabled
                && account.proxy_disabled_reason.as_ref().is_some_and(|r| r == "quota_protection")
            {
                logger::log_info(&format!(
                    "[Quota] Migrating account {} from account-level to model-level protection",
                    account.email
                ));
                account.enable_for_proxy();
            }
        }
    }

    save_account(&account)?;
    Ok(account)
}
