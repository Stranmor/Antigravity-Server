//! Circuit Breaker implementation for account-level fast-fail behavior
//!
//! This module provides a circuit breaker pattern to prevent repeated calls to
//! failing upstream services. When an account experiences multiple consecutive
//! failures, the circuit breaker opens and subsequent requests fail fast.
//!
//! States:
//! - Closed: Normal operation, requests pass through
//! - Open: Account is failing, requests fail immediately
//! - Half-Open: Testing if account has recovered

mod state;

mod recording;

#[cfg(test)]
mod tests;

use state::AccountCircuit;
pub use state::{CircuitBreakerConfig, CircuitBreakerSummary, CircuitState};

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::time::Duration;
use tracing::debug;

/// Manages circuit breakers for all accounts
#[derive(Debug)]
pub struct CircuitBreakerManager {
    pub(super) config: CircuitBreakerConfig,
    pub(super) circuits: RwLock<HashMap<String, AccountCircuit>>,
    pub(super) total_trips: AtomicU64,
}

impl Default for CircuitBreakerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreakerManager {
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self { config, circuits: RwLock::new(HashMap::new()), total_trips: AtomicU64::new(0) }
    }

    /// Check if an account's circuit is open (should fail fast)
    ///
    /// Returns `Some(reason)` if circuit is open, `None` if request should proceed
    pub fn check(&self, account_id: &str) -> Option<String> {
        let mut circuits = self.circuits.write();
        let circuit = circuits.entry(account_id.to_string()).or_default();

        match circuit.state {
            CircuitState::Open => {
                if let Some(opened_at) = circuit.opened_at {
                    let elapsed: Duration = opened_at.elapsed();
                    if elapsed >= self.config.open_duration {
                        debug!(
                            account_id = %account_id,
                            "Circuit breaker transitioning to half-open"
                        );
                        let previous_state = circuit.state;
                        circuit.state = CircuitState::HalfOpen;
                        circuit.consecutive_successes = 0;

                        Self::persist_state_change(
                            account_id,
                            previous_state,
                            CircuitState::HalfOpen,
                            Some("Timeout elapsed, testing recovery"),
                            None,
                        );

                        return None;
                    }
                }
                circuit.last_failure_reason.clone()
            },
            CircuitState::Closed => None,
            CircuitState::HalfOpen => {
                if circuit.half_open_probe_active {
                    Some("Circuit half-open: probe already in flight".to_string())
                } else {
                    circuit.half_open_probe_active = true;
                    None
                }
            },
        }
    }

    /// Check if request should be allowed (Result-based API for handlers)
    ///
    /// Returns `Ok(())` if request can proceed, `Err(Duration)` with retry delay if blocked
    pub fn should_allow(&self, account_id: &str) -> Result<(), Duration> {
        let mut circuits = self.circuits.write();
        let circuit = circuits.entry(account_id.to_string()).or_default();

        match circuit.state {
            CircuitState::Open => {
                if let Some(opened_at) = circuit.opened_at {
                    let elapsed: Duration = opened_at.elapsed();
                    if elapsed >= self.config.open_duration {
                        debug!(
                            account_id = %account_id,
                            "Circuit breaker transitioning to half-open"
                        );
                        let previous_state = circuit.state;
                        circuit.state = CircuitState::HalfOpen;
                        circuit.consecutive_successes = 0;

                        Self::persist_state_change(
                            account_id,
                            previous_state,
                            CircuitState::HalfOpen,
                            Some("Timeout elapsed, testing recovery"),
                            None,
                        );

                        return Ok(());
                    }
                    let remaining = self.config.open_duration.saturating_sub(elapsed);
                    return Err(remaining);
                }
                Err(self.config.open_duration)
            },
            CircuitState::Closed => Ok(()),
            CircuitState::HalfOpen => {
                if circuit.half_open_probe_active {
                    Err(Duration::from_secs(1))
                } else {
                    circuit.half_open_probe_active = true;
                    Ok(())
                }
            },
        }
    }
}
