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

#[cfg(test)]
mod tests;

use state::AccountCircuit;
pub use state::{CircuitBreakerConfig, CircuitBreakerSummary, CircuitState};

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Manages circuit breakers for all accounts
#[derive(Debug)]
pub struct CircuitBreakerManager {
    config: CircuitBreakerConfig,
    circuits: RwLock<HashMap<String, AccountCircuit>>,
    total_trips: AtomicU64,
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
            CircuitState::Closed | CircuitState::HalfOpen => None,
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
            CircuitState::Closed | CircuitState::HalfOpen => Ok(()),
        }
    }

    pub fn record_success(&self, account_id: &str) {
        let mut circuits = self.circuits.write();
        let circuit = circuits.entry(account_id.to_string()).or_default();

        match circuit.state {
            CircuitState::Closed => {
                circuit.consecutive_failures = 0;
            },
            CircuitState::HalfOpen => {
                circuit.consecutive_successes += 1;
                if circuit.consecutive_successes >= self.config.success_threshold {
                    info!(
                        account_id = %account_id,
                        "Circuit breaker closing - account recovered"
                    );
                    let previous_state = circuit.state;
                    circuit.state = CircuitState::Closed;
                    circuit.consecutive_failures = 0;
                    circuit.consecutive_successes = 0;
                    circuit.opened_at = None;
                    circuit.last_failure_reason = None;

                    Self::persist_state_change(
                        account_id,
                        previous_state,
                        CircuitState::Closed,
                        Some("Account recovered"),
                        None,
                    );
                }
            },
            CircuitState::Open => {
                debug!(
                    account_id = %account_id,
                    "Unexpected success in open state"
                );
            },
        }
    }

    pub fn record_failure(&self, account_id: &str, reason: &str) {
        let mut circuits = self.circuits.write();
        let circuit = circuits.entry(account_id.to_string()).or_default();

        let previous_state = circuit.state;
        circuit.consecutive_failures += 1;
        circuit.consecutive_successes = 0;
        circuit.last_failure_reason = Some(reason.to_string());

        match circuit.state {
            CircuitState::Closed => {
                if circuit.consecutive_failures >= self.config.failure_threshold {
                    warn!(
                        account_id = %account_id,
                        failures = circuit.consecutive_failures,
                        reason = %reason,
                        "Circuit breaker opening - too many failures"
                    );
                    circuit.state = CircuitState::Open;
                    circuit.opened_at = Some(std::time::Instant::now());
                    self.total_trips.fetch_add(1, Ordering::Relaxed);

                    Self::persist_state_change(
                        account_id,
                        previous_state,
                        CircuitState::Open,
                        Some(reason),
                        Some(i32::try_from(circuit.consecutive_failures).unwrap_or(i32::MAX)),
                    );
                }
            },
            CircuitState::HalfOpen => {
                warn!(
                    account_id = %account_id,
                    reason = %reason,
                    "Circuit breaker re-opening - failure during half-open"
                );
                circuit.state = CircuitState::Open;
                circuit.opened_at = Some(std::time::Instant::now());
                self.total_trips.fetch_add(1, Ordering::Relaxed);

                Self::persist_state_change(
                    account_id,
                    previous_state,
                    CircuitState::Open,
                    Some(reason),
                    Some(i32::try_from(circuit.consecutive_failures).unwrap_or(i32::MAX)),
                );
            },
            CircuitState::Open => {},
        }
    }

    fn persist_state_change(
        account_id: &str,
        previous_state: CircuitState,
        new_state: CircuitState,
        reason: Option<&str>,
        failure_count: Option<i32>,
    ) {
        let _ = failure_count;
        tracing::info!(
            "Circuit breaker state change: {} {:?} -> {:?} (reason: {:?})",
            account_id,
            previous_state,
            new_state,
            reason
        );
    }

    pub fn get_state(&self, account_id: &str) -> CircuitState {
        let circuits = self.circuits.read();
        circuits.get(account_id).map_or(CircuitState::Closed, |c| c.state)
    }

    pub fn total_trips(&self) -> u64 {
        self.total_trips.load(Ordering::Relaxed)
    }

    pub fn reset(&self, account_id: &str) {
        let mut circuits = self.circuits.write();
        if let Some(circuit) = circuits.get_mut(account_id) {
            let previous_state = circuit.state;
            info!(
                account_id = %account_id,
                previous_state = ?previous_state,
                "Circuit breaker reset manually"
            );

            if previous_state != CircuitState::Closed {
                Self::persist_state_change(
                    account_id,
                    previous_state,
                    CircuitState::Closed,
                    Some("Manual reset by user"),
                    None,
                );
            }

            *circuit = AccountCircuit::default();
        }
    }

    pub fn get_summary(&self) -> CircuitBreakerSummary {
        let circuits = self.circuits.read();
        let mut closed = 0;
        let mut open = 0;
        let mut half_open = 0;

        for circuit in circuits.values() {
            match circuit.state {
                CircuitState::Closed => closed += 1,
                CircuitState::Open => open += 1,
                CircuitState::HalfOpen => half_open += 1,
            }
        }

        CircuitBreakerSummary { closed, open, half_open, total_trips: self.total_trips() }
    }
}
