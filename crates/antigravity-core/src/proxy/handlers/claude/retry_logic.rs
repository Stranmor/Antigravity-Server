use tokio::time::{sleep, Duration};
use tracing::{debug, info};

pub const MAX_RETRY_ATTEMPTS: usize = 64;

const SIGNATURE_ERROR_PATTERNS: &[&str] = &[
    "Invalid `signature`",
    "Invalid signature",
    "thinking.signature",
    "thinking.thinking",
    "thinking.signature: Field required",
    "thinking.thinking: Field required",
    "INVALID_ARGUMENT",
    "Corrupted thought signature",
    "failed to deserialise",
    "thinking block",
    "Found `text`",
    "Found 'text'",
    "must be `thinking`",
    "must be 'thinking'",
];

#[derive(Debug, Clone)]
pub enum RetryStrategy {
    NoRetry,
    FixedDelay(Duration),
    LinearBackoff { base_ms: u64 },
    ExponentialBackoff { base_ms: u64, max_ms: u64 },
}

#[inline]
pub fn is_signature_error(error_text: &str) -> bool {
    SIGNATURE_ERROR_PATTERNS.iter().any(|pattern| error_text.contains(pattern))
}

pub fn determine_retry_strategy(
    status_code: u16,
    error_text: &str,
    retried_without_thinking: bool,
) -> RetryStrategy {
    match status_code {
        400 if !retried_without_thinking && is_signature_error(error_text) => {
            RetryStrategy::FixedDelay(Duration::from_millis(200))
        },

        429 => {
            if let Some(delay_ms) = crate::proxy::upstream::retry::parse_retry_delay(error_text) {
                let actual_delay = delay_ms.saturating_add(200).min(10_000);
                RetryStrategy::FixedDelay(Duration::from_millis(actual_delay))
            } else {
                RetryStrategy::LinearBackoff { base_ms: 1000 }
            }
        },

        503 | 529 => RetryStrategy::ExponentialBackoff { base_ms: 1000, max_ms: 8000 },

        500 => RetryStrategy::LinearBackoff { base_ms: 500 },

        401 | 403 => RetryStrategy::FixedDelay(Duration::from_millis(100)),

        _ => RetryStrategy::NoRetry,
    }
}

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
                "[{}] ⏱️  Retry with fixed delay: status={}, attempt={}/{}, base={}ms",
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
            let calculated_ms = base_ms * (attempt as u64 + 1);
            info!(
                "[{}] ⏱️  Retry with linear backoff: status={}, attempt={}/{}, base={}ms",
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
            let calculated_ms = (base_ms * 2_u64.pow(attempt as u32)).min(max_ms);
            info!(
                "[{}] ⏱️  Retry with exponential backoff: status={}, attempt={}/{}, base={}ms",
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

pub fn should_rotate_account(status_code: u16) -> bool {
    matches!(status_code, 429 | 401 | 403 | 500 | 503 | 529)
}
