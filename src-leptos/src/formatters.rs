//! Utility functions for formatting and display

use chrono::{DateTime, Utc};

/// Format a reset time string (ISO 8601) into human-readable remaining time.
///
/// Examples:
/// - "2026-01-19T05:30:00Z" -> "2h 15m" (if 2h 15m remaining)
/// - Past time -> "0h 0m"
/// - "1d 3h" for times > 24h
pub fn format_time_remaining(date_str: &str) -> String {
    if date_str.is_empty() {
        return "Unknown".to_string();
    }

    // Parse ISO 8601 datetime
    let target = match DateTime::parse_from_rfc3339(date_str) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => {
            // Try parsing without timezone (assume UTC)
            match date_str.parse::<DateTime<Utc>>() {
                Ok(dt) => dt,
                Err(_) => return date_str.to_string(), // Fallback: return as-is
            }
        }
    };

    let now = Utc::now();
    let diff = target.signed_duration_since(now);

    if diff.num_milliseconds() <= 0 {
        return "0h 0m".to_string();
    }

    let total_minutes = diff.num_minutes();
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;

    if hours >= 24 {
        let days = hours / 24;
        let remaining_hours = hours % 24;
        format!("{}d {}h", days, remaining_hours)
    } else {
        format!("{}h {}m", hours, minutes)
    }
}

/// Get CSS color class based on time remaining until reset.
///
/// - < 1h: "success" (green) - almost reset
/// - 1-6h: "warning" (amber) - waiting
/// - > 6h: "neutral" (gray) - long wait
/// - Already reset: "success"
pub fn get_time_remaining_color(date_str: &str) -> &'static str {
    if date_str.is_empty() {
        return "neutral";
    }

    let target = match DateTime::parse_from_rfc3339(date_str) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => match date_str.parse::<DateTime<Utc>>() {
            Ok(dt) => dt,
            Err(_) => return "neutral",
        },
    };

    let now = Utc::now();
    let diff = target.signed_duration_since(now);

    if diff.num_milliseconds() <= 0 {
        return "success"; // Already reset or about to reset
    }

    let hours = diff.num_hours();

    if hours < 1 {
        "success" // < 1h: green (almost reset)
    } else if hours < 6 {
        "warning" // 1-6h: amber (waiting)
    } else {
        "neutral" // > 6h: gray (long wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time_remaining_empty() {
        assert_eq!(format_time_remaining(""), "Unknown");
    }

    #[test]
    fn test_format_time_remaining_invalid() {
        assert_eq!(format_time_remaining("not a date"), "not a date");
    }
}
