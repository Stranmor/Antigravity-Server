mod error_parsing;
mod lockout;
mod parser;
mod rate_limit_info;
mod tracker;

pub use rate_limit_info::{RateLimitInfo, RateLimitKey, RateLimitReason};
pub use tracker::RateLimitTracker;

use std::time::Duration;

pub(crate) const FAILURE_COUNT_EXPIRY_SECONDS: u64 = 3600;

const QUOTA_LOCKOUT_TIER_1: u64 = 60;
const QUOTA_LOCKOUT_TIER_2: u64 = 300;
const QUOTA_LOCKOUT_TIER_3: u64 = 1800;
const QUOTA_LOCKOUT_TIER_4: u64 = 7200;

const RATE_LIMIT_DEFAULT_SECONDS: u64 = 5;

fn duration_to_secs_ceil(d: Duration) -> u64 {
    let secs = d.as_secs();
    if d.subsec_nanos() > 0 {
        secs + 1
    } else {
        secs
    }
}

#[cfg(test)]
mod tests;
