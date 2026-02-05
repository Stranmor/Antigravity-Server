//! SQLite-based proxy request logging and statistics.
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "timestamp conversions and statistics calculations"
)]

use antigravity_types::models::{ProxyRequestLog, ProxyStats, TokenUsageStats};
use rusqlite::{params, Connection};
use std::path::PathBuf;

/// Get the path to the proxy database file.
pub fn get_proxy_db_path() -> Result<PathBuf, String> {
    let data_dir = crate::utils::paths::get_data_dir()?;
    Ok(data_dir.join("proxy_logs.db"))
}

/// Initialize the proxy database schema.
pub fn init_db() -> Result<(), String> {
    let db_path = get_proxy_db_path()?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;

    let _rows_affected: usize = conn
        .execute(
            "CREATE TABLE IF NOT EXISTS request_logs (
            id TEXT PRIMARY KEY,
            timestamp INTEGER,
            method TEXT,
            url TEXT,
            status INTEGER,
            duration INTEGER,
            model TEXT,
            error TEXT,
            request_body TEXT,
            response_body TEXT,
            input_tokens INTEGER,
            output_tokens INTEGER,
            account_email TEXT,
            mapped_model TEXT,
            mapping_reason TEXT
        )",
            [],
        )
        .map_err(|err| err.to_string())?;

    drop(conn.execute("ALTER TABLE request_logs ADD COLUMN request_body TEXT", []));
    drop(conn.execute("ALTER TABLE request_logs ADD COLUMN response_body TEXT", []));
    drop(conn.execute("ALTER TABLE request_logs ADD COLUMN input_tokens INTEGER", []));
    drop(conn.execute("ALTER TABLE request_logs ADD COLUMN output_tokens INTEGER", []));
    drop(conn.execute("ALTER TABLE request_logs ADD COLUMN account_email TEXT", []));
    drop(conn.execute("ALTER TABLE request_logs ADD COLUMN mapped_model TEXT", []));
    drop(conn.execute("ALTER TABLE request_logs ADD COLUMN mapping_reason TEXT", []));
    drop(conn.execute("ALTER TABLE request_logs ADD COLUMN cached_tokens INTEGER", []));

    let _rows_affected: usize = conn
        .execute("CREATE INDEX IF NOT EXISTS idx_timestamp ON request_logs (timestamp DESC)", [])
        .map_err(|err| err.to_string())?;

    Ok(())
}

/// Save a request log entry to the database.
pub fn save_log(log: &ProxyRequestLog) -> Result<(), String> {
    let db_path = get_proxy_db_path()?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;

    let _rows_affected: usize = conn
        .execute(
            "INSERT INTO request_logs (id, timestamp, method, url, status, duration, model, error, request_body, response_body, input_tokens, output_tokens, account_email, mapped_model, mapping_reason, cached_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                log.id,
                log.timestamp,
                log.method,
                log.url,
                log.status,
                log.duration,
                log.model,
                log.error,
                log.request_body,
                log.response_body,
                log.input_tokens,
                log.output_tokens,
                log.account_email,
                log.mapped_model,
                log.mapping_reason,
                log.cached_tokens,
            ],
        )
        .map_err(|err| err.to_string())?;

    Ok(())
}

/// Get recent request logs from the database.
pub fn get_logs(limit: usize) -> Result<Vec<ProxyRequestLog>, String> {
    let db_path = get_proxy_db_path()?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;

    let mut stmt = conn.prepare(
        "SELECT id, timestamp, method, url, status, duration, model, error, request_body, response_body, input_tokens, output_tokens, account_email, mapped_model, mapping_reason, cached_tokens
         FROM request_logs
         ORDER BY timestamp DESC
         LIMIT ?1"
    ).map_err(|err| err.to_string())?;

    let logs_iter = stmt
        .query_map([limit], |row| {
            Ok(ProxyRequestLog {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                method: row.get(2)?,
                url: row.get(3)?,
                status: row.get(4)?,
                duration: row.get(5)?,
                model: row.get(6)?,
                mapped_model: row.get(13).unwrap_or(None),
                mapping_reason: row.get(14).unwrap_or(None),
                account_email: row.get(12).unwrap_or(None),
                error: row.get(7)?,
                request_body: row.get(8).unwrap_or(None),
                response_body: row.get(9).unwrap_or(None),
                input_tokens: row.get(10).unwrap_or(None),
                output_tokens: row.get(11).unwrap_or(None),
                cached_tokens: row.get(15).unwrap_or(None),
            })
        })
        .map_err(|err| err.to_string())?;

    let mut logs = Vec::new();
    for log in logs_iter {
        logs.push(log.map_err(|err| err.to_string())?);
    }
    Ok(logs)
}

/// Get aggregate statistics from the proxy logs.
pub fn get_stats() -> Result<ProxyStats, String> {
    let db_path = get_proxy_db_path()?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;

    let total_requests: u64 = conn
        .query_row("SELECT COUNT(*) FROM request_logs", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;

    let success_count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM request_logs WHERE status >= 200 AND status < 400",
            [],
            |row| row.get(0),
        )
        .map_err(|err| err.to_string())?;

    let error_count: u64 = conn
        .query_row(
            "SELECT COUNT(*) FROM request_logs WHERE status < 200 OR status >= 400",
            [],
            |row| row.get(0),
        )
        .map_err(|err| err.to_string())?;

    let total_input_tokens: u64 = conn
        .query_row("SELECT COALESCE(SUM(input_tokens), 0) FROM request_logs", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;

    let total_output_tokens: u64 = conn
        .query_row("SELECT COALESCE(SUM(output_tokens), 0) FROM request_logs", [], |row| row.get(0))
        .map_err(|err| err.to_string())?;

    Ok(ProxyStats {
        total_requests,
        success_count,
        error_count,
        total_input_tokens,
        total_output_tokens,
    })
}

/// Clear all proxy logs from the database.
pub fn clear_proxy_logs() -> Result<(), String> {
    let db_path = get_proxy_db_path()?;
    let conn = Connection::open(db_path).map_err(|err| err.to_string())?;
    let _rows_affected: usize =
        conn.execute("DELETE FROM request_logs", []).map_err(|err| err.to_string())?;
    Ok(())
}

/// Get token usage statistics over time.
pub fn get_token_usage_stats() -> Result<TokenUsageStats, String> {
    let db_path = get_proxy_db_path()?;
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
