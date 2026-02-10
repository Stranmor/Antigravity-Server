//! Unified retry strategy for all protocol handlers.
//!
//! Provides protocol-specific backoff profiles (OpenAI, Claude, Gemini)
//! with a single `determine_retry_strategy()` entry point.

mod peek;
mod profile;

pub use peek::{peek_first_data_chunk, PeekConfig, PeekResult};
pub use profile::RetryProfile;

use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info};

/// Maximum retry attempts before giving up. Capped by pool size at call site.
pub const MAX_RETRY_ATTEMPTS: usize = 64;

/// HTTP status codes that warrant rotating to a different account.
pub const ROTATABLE_STATUS_CODES: &[u16] = &[429, 401, 403, 404, 500, 503, 529];

/// HTTP status codes indicating rate limiting (subset used for mark_rate_limited).
pub const RATE_LIMIT_CODES: &[u16] = &[429, 529, 503, 500];

/// Delay to prevent thundering herd when all accounts are temporarily limited.
pub const THUNDERING_HERD_DELAY: Duration = Duration::from_millis(500);

/// Strategy for retrying failed upstream requests.
#[derive(Debug, Clone)]
pub enum RetryStrategy {
    /// Do not retry.
    NoRetry,
    /// Retry after a fixed delay.
    FixedDelay(Duration),
    /// Retry with linearly increasing delay.
    LinearBackoff {
        /// Base delay in milliseconds.
        base_ms: u64,
    },
    /// Retry with exponentially increasing delay.
    ExponentialBackoff {
        /// Base delay in milliseconds.
        base_ms: u64,
        /// Maximum delay in milliseconds.
        max_ms: u64,
    },
}

/// Checks whether the error text matches a known signature/thinking error.
#[inline]
pub fn is_signature_error(error_text: &str, profile: &RetryProfile) -> bool {
    profile.signature_patterns.iter().any(|p| error_text.contains(p))
}

/// Determines the appropriate retry strategy based on status code and profile.
pub fn determine_retry_strategy(
    status_code: u16,
    error_text: &str,
    retried_without_thinking: bool,
    profile: &RetryProfile,
) -> RetryStrategy {
    match status_code {
        400 if !retried_without_thinking && is_signature_error(error_text, profile) => {
            RetryStrategy::FixedDelay(Duration::from_millis(profile.fixed_401_403_delay_ms))
        },
        429 => {
            if let Some(delay_ms) = crate::proxy::upstream::retry::parse_retry_delay(error_text) {
                let actual_delay = delay_ms.saturating_add(200).min(profile.backoff_429_max_ms);
                RetryStrategy::FixedDelay(Duration::from_millis(actual_delay))
            } else {
                RetryStrategy::LinearBackoff { base_ms: profile.backoff_429_base_ms }
            }
        },
        503 | 529 => RetryStrategy::ExponentialBackoff {
            base_ms: profile.backoff_503_base_ms,
            max_ms: profile.backoff_503_max_ms,
        },
        500 => RetryStrategy::LinearBackoff { base_ms: profile.backoff_500_base_ms },
        401 | 403 => {
            RetryStrategy::FixedDelay(Duration::from_millis(profile.fixed_401_403_delay_ms))
        },
        404 => RetryStrategy::FixedDelay(Duration::from_millis(100)),
        _ => RetryStrategy::NoRetry,
    }
}

/// Applies the retry strategy, sleeping the appropriate duration.
///
/// Returns `true` if retry should proceed, `false` if we should stop.
pub async fn apply_retry_strategy(
    strategy: RetryStrategy,
    attempt: usize,
    status_code: u16,
    trace_id: &str,
) -> bool {
    match strategy {
        RetryStrategy::NoRetry => {
            debug!("[{}] Non-retryable error {}, stopping", trace_id, status_code);
            false
        },
        RetryStrategy::FixedDelay(duration) => {
            let base_ms = duration.as_millis() as u64;
            info!(
                "[{}] Retry with fixed delay: status={}, attempt={}/{}, delay={}ms",
                trace_id,
                status_code,
                attempt + 1,
                MAX_RETRY_ATTEMPTS,
                base_ms
            );
            sleep(duration).await;
            true
        },
        RetryStrategy::LinearBackoff { base_ms } => {
            let calculated_ms = base_ms.saturating_mul(attempt as u64 + 1);
            info!(
                "[{}] Retry with linear backoff: status={}, attempt={}/{}, delay={}ms",
                trace_id,
                status_code,
                attempt + 1,
                MAX_RETRY_ATTEMPTS,
                calculated_ms
            );
            sleep(Duration::from_millis(calculated_ms)).await;
            true
        },
        RetryStrategy::ExponentialBackoff { base_ms, max_ms } => {
            let calculated_ms =
                base_ms.saturating_mul(2_u64.saturating_pow(attempt as u32)).min(max_ms);
            info!(
                "[{}] Retry with exponential backoff: status={}, attempt={}/{}, delay={}ms",
                trace_id,
                status_code,
                attempt + 1,
                MAX_RETRY_ATTEMPTS,
                calculated_ms
            );
            sleep(Duration::from_millis(calculated_ms)).await;
            true
        },
    }
}

/// Checks if the status code warrants rotating to a different account.
///
/// Includes 503/529 (the bug fix: OpenAI handler previously missed these).
pub fn should_rotate_account(status_code: u16) -> bool {
    ROTATABLE_STATUS_CODES.contains(&status_code)
}

/// Checks if the status code indicates a rate-limiting condition.
pub fn is_rate_limit_code(status_code: u16) -> bool {
    RATE_LIMIT_CODES.contains(&status_code)
}
