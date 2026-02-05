use dashmap::DashMap;
use std::net::IpAddr;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

const MAX_FAILED_ATTEMPTS: u32 = 5;
const BLOCK_DURATION: Duration = Duration::from_secs(15 * 60);
const CLEANUP_THRESHOLD: usize = 1000;

struct FailedAttempt {
    count: u32,
    blocked_until: Option<Instant>,
}

static RATE_LIMITER: LazyLock<DashMap<IpAddr, FailedAttempt>> = LazyLock::new(DashMap::new);

pub fn is_blocked(ip: IpAddr) -> bool {
    if let Some(entry) = RATE_LIMITER.get(&ip) {
        if let Some(blocked_until) = entry.blocked_until {
            if Instant::now() < blocked_until {
                return true;
            }
        }
    }
    false
}

pub fn record_failed_attempt(ip: IpAddr) -> bool {
    cleanup_if_needed();

    let now = Instant::now();
    let mut entry =
        RATE_LIMITER.entry(ip).or_insert(FailedAttempt { count: 0, blocked_until: None });

    if entry.blocked_until.is_some_and(|t| now >= t) {
        entry.count = 0;
        entry.blocked_until = None;
    }

    entry.count = entry.count.saturating_add(1);

    if entry.count >= MAX_FAILED_ATTEMPTS {
        entry.blocked_until = now.checked_add(BLOCK_DURATION);
        tracing::warn!(
            "IP {} blocked for 15 minutes after {} failed auth attempts",
            ip,
            entry.count
        );
        return true;
    }

    false
}

pub fn clear_failed_attempts(ip: IpAddr) {
    RATE_LIMITER.remove(&ip);
}

fn cleanup_if_needed() {
    if RATE_LIMITER.len() > CLEANUP_THRESHOLD {
        let now = Instant::now();
        RATE_LIMITER.retain(|_, v| v.blocked_until.is_some_and(|t| now < t) || v.count > 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_rate_limiting() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        clear_failed_attempts(ip);

        assert!(!is_blocked(ip));

        for _ in 0..4 {
            assert!(!record_failed_attempt(ip));
            assert!(!is_blocked(ip));
        }

        assert!(record_failed_attempt(ip));
        assert!(is_blocked(ip));

        clear_failed_attempts(ip);
        assert!(!is_blocked(ip));
    }
}
