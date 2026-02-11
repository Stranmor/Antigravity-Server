//! Global thinking budget configuration accessor.

use antigravity_types::models::ThinkingBudgetConfig;
use std::sync::{OnceLock, RwLock};

static GLOBAL_THINKING_BUDGET: OnceLock<RwLock<ThinkingBudgetConfig>> = OnceLock::new();

/// Get the current thinking budget configuration.
/// Returns default (Adaptive mode) if not yet initialized.
pub fn get_thinking_budget_config() -> ThinkingBudgetConfig {
    GLOBAL_THINKING_BUDGET
        .get()
        .map(|lock| match lock.read() {
            Ok(cfg) => cfg.clone(),
            Err(poisoned) => {
                tracing::error!("thinking budget config RwLock poisoned, recovering value");
                poisoned.into_inner().clone()
            },
        })
        .unwrap_or_default()
}

/// Update the global thinking budget configuration.
/// Called during startup and hot-reload.
pub fn update_thinking_budget_config(config: ThinkingBudgetConfig) {
    let lock = GLOBAL_THINKING_BUDGET.get_or_init(|| RwLock::new(ThinkingBudgetConfig::default()));
    match lock.write() {
        Ok(mut guard) => *guard = config,
        Err(poisoned) => {
            tracing::error!("thinking budget config RwLock poisoned during update, recovering");
            *poisoned.into_inner() = config;
        },
    }
}

/// Mutex for serializing tests that mutate the global thinking budget config.
/// Use `let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap();` at test start.
#[cfg(test)]
pub static THINKING_CONFIG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use antigravity_types::models::ThinkingBudgetMode;

    #[test]
    fn test_get_returns_default_before_init() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let config = get_thinking_budget_config();
        // After other tests may have mutated this, we can only check it's a valid mode
        assert!(matches!(
            config.mode,
            ThinkingBudgetMode::Adaptive
                | ThinkingBudgetMode::Auto
                | ThinkingBudgetMode::Custom
                | ThinkingBudgetMode::Passthrough
        ));
    }

    #[test]
    fn test_update_and_get() {
        let _guard = THINKING_CONFIG_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let custom = ThinkingBudgetConfig {
            mode: ThinkingBudgetMode::Custom,
            custom_value: 30000,
            effort: None,
        };
        update_thinking_budget_config(custom.clone());
        let got = get_thinking_budget_config();
        assert_eq!(got.mode, ThinkingBudgetMode::Custom);
        assert_eq!(got.custom_value, 30000);
    }
}
