#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "timestamp conversions and statistics calculations"
)]

use antigravity_types::models::TokenUsageStats;
use rusqlite::Connection;

fn get_token_usage_stats_sync() -> Result<TokenUsageStats, String> {
    let db_path = super::proxy_db::get_proxy_db_path()?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| err.to_string())?
        .as_secs() as i64;

    let hour_ago = now.saturating_sub(3600);
    let day_ago = now.saturating_sub(86400);

    let (total_input, total_output, total_cached, total_requests, min_ts, max_ts): (
        u64, u64, u64, u64, Option<i64>, Option<i64>,
    ) = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0), \
             COALESCE(SUM(cached_tokens), 0), COUNT(*), MIN(timestamp), MAX(timestamp) FROM request_logs",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )
        .map_err(|err| err.to_string())?;

    let (requests_last_hour, tokens_last_hour, cached_last_hour): (u64, u64, u64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0) + COALESCE(SUM(output_tokens), 0), \
             COALESCE(SUM(cached_tokens), 0) FROM request_logs WHERE timestamp >= ?1",
            [hour_ago],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|err| err.to_string())?;

    let (requests_last_24h, tokens_last_24h, cached_last_24h): (u64, u64, u64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(input_tokens), 0) + COALESCE(SUM(output_tokens), 0), \
             COALESCE(SUM(cached_tokens), 0) FROM request_logs WHERE timestamp >= ?1",
            [day_ago],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|err| err.to_string())?;

    #[allow(
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "safe cast: time difference is always positive when max > min"
    )]
    let time_range_secs = match (min_ts, max_ts) {
        (Some(min), Some(max)) if max > min => (max.saturating_sub(min)) as u64,
        (Some(_) | None, Some(_) | None) => 1,
    };

    let total_tokens = total_input.saturating_add(total_output);
    let total_context = total_input.saturating_add(total_cached);

    #[allow(
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "intentional precision loss for human-readable statistics display"
    )]
    let minutes = (time_range_secs as f64) / 60.0;
    #[allow(
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "intentional precision loss for human-readable statistics display"
    )]
    let hours = (time_range_secs as f64) / 3600.0;
    #[allow(
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "intentional precision loss for human-readable statistics display"
    )]
    let days = (time_range_secs as f64) / 86400.0;

    #[allow(
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "intentional precision loss for percentage calculation"
    )]
    let cache_hit_rate =
        if total_context > 0 { (total_cached as f64 / total_context as f64) * 100.0 } else { 0.0 };

    #[allow(
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "intentional precision loss for average calculations"
    )]
    let (avg_input_per_request, avg_output_per_request, avg_cached_per_request) =
        if total_requests > 0 {
            (
                total_input as f64 / total_requests as f64,
                total_output as f64 / total_requests as f64,
                total_cached as f64 / total_requests as f64,
            )
        } else {
            (0.0, 0.0, 0.0)
        };

    #[allow(
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "intentional precision loss for rate calculations"
    )]
    let (avg_tokens_per_minute, avg_tokens_per_hour, avg_tokens_per_day) = (
        if minutes > 0.0 { total_tokens as f64 / minutes } else { 0.0 },
        if hours > 0.0 { total_tokens as f64 / hours } else { 0.0 },
        if days > 0.0 { total_tokens as f64 / days } else { 0.0 },
    );

    Ok(TokenUsageStats {
        total_input,
        total_output,
        total_cached,
        total_requests,
        time_range_secs,
        avg_input_per_request,
        avg_output_per_request,
        avg_cached_per_request,
        avg_tokens_per_minute,
        avg_tokens_per_hour,
        avg_tokens_per_day,
        requests_last_hour,
        tokens_last_hour,
        cached_last_hour,
        requests_last_24h,
        tokens_last_24h,
        cached_last_24h,
        cache_hit_rate,
    })
}

pub async fn get_token_usage_stats() -> Result<TokenUsageStats, String> {
    tokio::task::spawn_blocking(get_token_usage_stats_sync)
        .await
        .map_err(|e| format!("spawn_blocking panicked: {e}"))?
}
