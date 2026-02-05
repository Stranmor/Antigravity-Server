//! Formatting utilities for monitor page
#![allow(
    clippy::integer_division,
    clippy::modulo_arithmetic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "time/date calculations require integer division and modulo"
)]

pub(crate) fn format_timestamp(ts: i64) -> String {
    // Backend stores timestamp in MILLISECONDS, convert to seconds first
    let ts_secs = ts / 1000;
    let secs = ts_secs % 86400;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, mins, s)
}

pub(crate) fn format_timestamp_full(ts: i64) -> String {
    // Backend stores timestamp in MILLISECONDS, convert to seconds first
    let ts_secs = ts / 1000;
    let days_since_epoch = ts_secs / 86400;
    let secs = ts_secs % 86400;
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    let s = secs % 60;

    // Simple date calculation (approximate, good enough for display)
    let year = 1970 + (days_since_epoch / 365);
    let day_of_year = days_since_epoch % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hours, mins, s)
}

pub(crate) fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}
