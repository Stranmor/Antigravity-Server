use std::time::{SystemTime, UNIX_EPOCH};

use super::types::{AccountHealth, AccountHealthResponse, HealthStatus};

pub async fn build_health_response(
    health: &AccountHealth,
    cooldown_secs: u64,
) -> AccountHealthResponse {
    let consecutive_errors = health.consecutive_errors();
    let is_disabled = health.is_disabled();
    let total_successes = health.total_successes();
    let total_errors = health.total_errors();

    let status = if is_disabled {
        let disabled_at = health.disabled_at.read().await;
        if disabled_at.is_some() {
            HealthStatus::Recovering
        } else {
            HealthStatus::Disabled
        }
    } else if consecutive_errors > 0 {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    };

    let (disabled_at_unix, cooldown_remaining) = {
        let disabled_at = health.disabled_at.read().await;
        if let Some(instant) = *disabled_at {
            let elapsed = instant.elapsed().as_secs();
            let remaining = cooldown_secs.saturating_sub(elapsed);

            let unix_ts =
                SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() - elapsed).ok();

            (unix_ts, Some(remaining))
        } else {
            (None, None)
        }
    };

    let last_error_type = *health.last_error_type.read().await;
    let last_error_message = health.last_error_message.read().await.clone();

    let total = total_successes + total_errors;
    let success_rate =
        if total > 0 { (f64::from(total_successes) / f64::from(total)) * 100.0 } else { 100.0 };

    AccountHealthResponse {
        account_id: health.account_id.clone(),
        email: health.email.clone(),
        status,
        consecutive_errors,
        is_disabled,
        disabled_at_unix,
        cooldown_remaining_seconds: cooldown_remaining,
        last_error_type,
        last_error_message,
        total_successes,
        total_errors,
        success_rate,
    }
}
