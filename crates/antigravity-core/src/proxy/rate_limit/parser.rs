use regex::Regex;
use std::sync::OnceLock;

static DURATION_REGEX: OnceLock<Regex> = OnceLock::new();
static RETRY_M_S_REGEX: OnceLock<Regex> = OnceLock::new();
static RETRY_S_REGEX: OnceLock<Regex> = OnceLock::new();
static QUOTA_RESET_REGEX: OnceLock<Regex> = OnceLock::new();
static RETRY_AFTER_REGEX: OnceLock<Regex> = OnceLock::new();
static WAIT_PAREN_REGEX: OnceLock<Regex> = OnceLock::new();

pub fn get_duration_regex() -> &'static Regex {
    DURATION_REGEX.get_or_init(|| {
        Regex::new(r"(?:(\d+)\s*h)?\s*(?:(\d+)\s*m)?\s*(?:(\d+(?:\.\d+)?)\s*s)?\s*(?:(\d+(?:\.\d+)?)\s*ms)?")
            .expect("Duration regex is valid")
    })
}

pub fn get_retry_m_s_regex() -> &'static Regex {
    RETRY_M_S_REGEX.get_or_init(|| {
        Regex::new(r"(?i)try again in (\d+)m\s*(\d+)s").expect("Retry m s regex is valid")
    })
}

pub fn get_retry_s_regex() -> &'static Regex {
    RETRY_S_REGEX.get_or_init(|| {
        Regex::new(r"(?i)(?:try again in|backoff for|wait)\s*(\d+)s")
            .expect("Retry s regex is valid")
    })
}

pub fn get_quota_reset_regex() -> &'static Regex {
    QUOTA_RESET_REGEX.get_or_init(|| {
        Regex::new(r"(?i)quota will reset in (\d+) second").expect("Quota reset regex is valid")
    })
}

pub fn get_retry_after_regex() -> &'static Regex {
    RETRY_AFTER_REGEX.get_or_init(|| {
        Regex::new(r"(?i)retry after (\d+) second").expect("Retry after regex is valid")
    })
}

pub fn get_wait_paren_regex() -> &'static Regex {
    WAIT_PAREN_REGEX
        .get_or_init(|| Regex::new(r"\(wait (\d+)s\)").expect("Wait paren regex is valid"))
}

pub fn parse_duration_string(s: &str) -> Option<u64> {
    tracing::debug!("[timeparse] attemptparse: '{}'", s);

    let re = get_duration_regex();
    let caps = match re.captures(s) {
        Some(c) => c,
        None => {
            tracing::warn!("[timeparse] regex did not match: '{}'", s);
            return None;
        },
    };

    let hours = caps.get(1).and_then(|m| m.as_str().parse::<u64>().ok()).unwrap_or(0);
    let minutes = caps.get(2).and_then(|m| m.as_str().parse::<u64>().ok()).unwrap_or(0);
    let seconds = caps.get(3).and_then(|m| m.as_str().parse::<f64>().ok()).unwrap_or(0.0);
    let milliseconds = caps.get(4).and_then(|m| m.as_str().parse::<f64>().ok()).unwrap_or(0.0);

    let any_matched = caps.get(1).is_some()
        || caps.get(2).is_some()
        || caps.get(3).is_some()
        || caps.get(4).is_some();

    if !any_matched {
        tracing::warn!("[timeparse] failed: '{}' (no matching components)", s);
        return None;
    }

    tracing::debug!(
        "[timeparse] extractresult: {}h {}m {:.3}s {:.3}ms",
        hours,
        minutes,
        seconds,
        milliseconds
    );

    let total_seconds =
        hours * 3600 + minutes * 60 + seconds.ceil() as u64 + (milliseconds / 1000.0).ceil() as u64;

    tracing::info!(
        "[timeparse] ✓ success: '{}' => {}second ({}h {}m {:.1}s {:.1}ms)",
        s,
        total_seconds,
        hours,
        minutes,
        seconds,
        milliseconds
    );
    Some(total_seconds)
}

pub fn parse_retry_time_from_body(body: &str) -> Option<u64> {
    let trimmed = body.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Some(delay_str) = json
                .get("error")
                .and_then(|e| e.get("details"))
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                .and_then(|o| o.get("metadata"))
                .and_then(|m| m.get("quotaResetDelay"))
                .and_then(|v| v.as_str())
            {
                tracing::debug!("[JSONparse] found quotaResetDelay: '{}'", delay_str);
                if let Some(seconds) = parse_duration_string(delay_str) {
                    return Some(seconds);
                }
            }

            if let Some(retry) =
                json.get("error").and_then(|e| e.get("retry_after")).and_then(|v| v.as_u64())
            {
                return Some(retry);
            }
        }
    }

    if let Some(caps) = get_retry_m_s_regex().captures(body) {
        if let (Ok(m), Ok(s)) = (caps[1].parse::<u64>(), caps[2].parse::<u64>()) {
            return Some(m * 60 + s);
        }
    }

    if let Some(caps) = get_retry_s_regex().captures(body) {
        if let Ok(s) = caps[1].parse::<u64>() {
            return Some(s);
        }
    }

    if let Some(caps) = get_quota_reset_regex().captures(body) {
        if let Ok(s) = caps[1].parse::<u64>() {
            return Some(s);
        }
    }

    if let Some(caps) = get_retry_after_regex().captures(body) {
        if let Ok(s) = caps[1].parse::<u64>() {
            return Some(s);
        }
    }

    if let Some(caps) = get_wait_paren_regex().captures(body) {
        if let Ok(s) = caps[1].parse::<u64>() {
            return Some(s);
        }
    }

    None
}
