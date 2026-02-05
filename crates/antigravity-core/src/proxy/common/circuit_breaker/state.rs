//! Circuit breaker state types and configuration

use std::time::{Duration, Instant};

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening the circuit
    pub failure_threshold: u32,
    /// Duration to keep circuit open before trying half-open
    pub open_duration: Duration,
    /// Number of successful requests in half-open state to close circuit
    pub success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self { failure_threshold: 5, open_duration: Duration::from_secs(60), success_threshold: 2 }
    }
}

/// State of the circuit breaker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests pass through
    Closed,
    /// Account is failing - requests fail immediately
    Open,
    /// Testing recovery - limited requests allowed
    HalfOpen,
}

/// Per-account circuit breaker state
#[derive(Debug)]
pub(crate) struct AccountCircuit {
    pub state: CircuitState,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub opened_at: Option<Instant>,
    pub last_failure_reason: Option<String>,
}

impl Default for AccountCircuit {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            opened_at: None,
            last_failure_reason: None,
        }
    }
}

/// Summary of circuit breaker states across all accounts
#[derive(Debug, Clone)]
pub struct CircuitBreakerSummary {
    pub closed: usize,
    pub open: usize,
    pub half_open: usize,
    pub total_trips: u64,
}
