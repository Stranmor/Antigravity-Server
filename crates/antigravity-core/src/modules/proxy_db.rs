//! SQLite-based proxy request logging and statistics.
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "timestamp conversions and statistics calculations"
)]

use antigravity_types::models::{ProxyRequestLog, ProxyStats};
use rusqlite::{params, Connection, Error as SqliteError};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;

thread_local! {
    static PROXY_DB_CONN: RefCell<Option<Connection>> = const { RefCell::new(None) };
    static INSERT_COUNT: Cell<u32> = const { Cell::new(0) };
}

/// Get the path to the proxy database file.
pub fn get_proxy_db_path() -> Result<PathBuf, String> {
    let data_dir = crate::utils::paths::get_data_dir()?;
    Ok(data_dir.join("proxy_logs.db"))
}

fn with_connection<T>(f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
    let db_path = get_proxy_db_path()?;
    PROXY_DB_CONN.with(|cell| {
        let needs_init = cell.borrow().is_none();
        if needs_init {
            let conn = Connection::open(&db_path).map_err(|err| err.to_string())?;
            *cell.borrow_mut() = Some(conn);
        }
        let conn_ref = cell.borrow();
        let conn = conn_ref.as_ref().ok_or_else(|| "Proxy DB connection missing".to_string())?;
        f(conn)
    })
}

fn add_column_if_missing(conn: &Connection, statement: &str) -> Result<(), String> {
    match conn.execute(statement, []) {
        Ok(_) => Ok(()),
        Err(SqliteError::SqliteFailure(_, Some(message)))
            if message.contains("duplicate column name") =>
        {
            Ok(())
        },
        Err(err) => Err(err.to_string()),
    }
}

/// Initialize the proxy database schema.
pub fn init_db() -> Result<(), String> {
    with_connection(|conn| {
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
                mapping_reason TEXT,
                cached_tokens INTEGER
            )",
                [],
            )
            .map_err(|err| err.to_string())?;

        add_column_if_missing(conn, "ALTER TABLE request_logs ADD COLUMN request_body TEXT")?;
        add_column_if_missing(conn, "ALTER TABLE request_logs ADD COLUMN response_body TEXT")?;
        add_column_if_missing(conn, "ALTER TABLE request_logs ADD COLUMN input_tokens INTEGER")?;
        add_column_if_missing(conn, "ALTER TABLE request_logs ADD COLUMN output_tokens INTEGER")?;
        add_column_if_missing(conn, "ALTER TABLE request_logs ADD COLUMN account_email TEXT")?;
        add_column_if_missing(conn, "ALTER TABLE request_logs ADD COLUMN mapped_model TEXT")?;
        add_column_if_missing(conn, "ALTER TABLE request_logs ADD COLUMN mapping_reason TEXT")?;
        add_column_if_missing(conn, "ALTER TABLE request_logs ADD COLUMN cached_tokens INTEGER")?;

        let _rows_affected: usize = conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_timestamp ON request_logs (timestamp DESC)",
                [],
            )
            .map_err(|err| err.to_string())?;

        Ok(())
    })
}

/// Save a request log entry to the database.
pub fn save_log(log: &ProxyRequestLog) -> Result<(), String> {
    with_connection(|conn| {
        let request_body: Option<String> = None;
        let response_body: Option<String> = None;
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
                    request_body,
                    response_body,
                    log.input_tokens,
                    log.output_tokens,
                    log.account_email,
                    log.mapped_model,
                    log.mapping_reason,
                    log.cached_tokens,
                ],
            )
            .map_err(|err| err.to_string())?;

        // Periodic cleanup
        INSERT_COUNT.with(|count| {
            let current = count.get() + 1;
            if current >= 1000 {
                count.set(0);
                let _ = cleanup_old_logs(7);
            } else {
                count.set(current);
            }
        });

        Ok(())
    })
}

/// Delete logs older than the given number of days.
pub fn cleanup_old_logs(retention_days: u32) -> Result<usize, String> {
    with_connection(|conn| {
        let cutoff = chrono::Utc::now().timestamp_millis() - i64::from(retention_days) * 86_400_000;
        let deleted: usize = conn
            .execute("DELETE FROM request_logs WHERE timestamp < ?1", params![cutoff])
            .map_err(|err| err.to_string())?;
        Ok(deleted)
    })
}

/// Get recent request logs from the database.
pub fn get_logs(limit: usize) -> Result<Vec<ProxyRequestLog>, String> {
    with_connection(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, timestamp, method, url, status, duration, model, error, request_body, response_body, input_tokens, output_tokens, account_email, mapped_model, mapping_reason, cached_tokens
             FROM request_logs
             ORDER BY timestamp DESC
             LIMIT ?1",
            )
            .map_err(|err| err.to_string())?;

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
    })
}

/// Get aggregate statistics from the proxy logs.
pub fn get_stats() -> Result<ProxyStats, String> {
    with_connection(|conn| {
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
            .query_row("SELECT COALESCE(SUM(input_tokens), 0) FROM request_logs", [], |row| {
                row.get(0)
            })
            .map_err(|err| err.to_string())?;

        let total_output_tokens: u64 = conn
            .query_row("SELECT COALESCE(SUM(output_tokens), 0) FROM request_logs", [], |row| {
                row.get(0)
            })
            .map_err(|err| err.to_string())?;

        Ok(ProxyStats {
            total_requests,
            success_count,
            error_count,
            total_input_tokens,
            total_output_tokens,
        })
    })
}

/// Clear all proxy logs from the database.
pub fn clear_proxy_logs() -> Result<(), String> {
    with_connection(|conn| {
        let _rows_affected: usize =
            conn.execute("DELETE FROM request_logs", []).map_err(|err| err.to_string())?;
        Ok(())
    })
}

pub use super::token_usage_stats::get_token_usage_stats;
