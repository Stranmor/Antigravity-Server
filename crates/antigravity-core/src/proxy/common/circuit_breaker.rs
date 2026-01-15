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

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

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
        Self {
            failure_threshold: 5,
            open_duration: Duration::from_secs(60),
            success_threshold: 2,
        }
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
struct AccountCircuit {
    state: CircuitState,
    consecutive_failures: u32,
    consecutive_successes: u32,
    opened_at: Option<Instant>,
    last_failure_reason: Option<String>,
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

/// Manages circuit breakers for all accounts
#[derive(Debug)]
pub struct CircuitBreakerManager {
    config: CircuitBreakerConfig,
    circuits: RwLock<HashMap<String, AccountCircuit>>,
    /// Total trips (circuit opens) for monitoring
    total_trips: AtomicU64,
}

impl Default for CircuitBreakerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreakerManager {
    /// Create a new circuit breaker manager with default config
    pub fn new() -> Self {
        Self::with_config(CircuitBreakerConfig::default())
    }

    /// Create a new circuit breaker manager with custom config
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            circuits: RwLock::new(HashMap::new()),
            total_trips: AtomicU64::new(0),
        }
    }

    /// Check if an account's circuit is open (should fail fast)
    ///
    /// Returns `Some(reason)` if circuit is open, `None` if request should proceed
    pub fn check(&self, account_id: &str) -> Option<String> {
        let mut circuits = self.circuits.write();
        let circuit = circuits.entry(account_id.to_string()).or_default();

        match circuit.state {
            CircuitState::Open => {
                // Check if we should transition to half-open
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

                        // Persist transition to half-open
                        Self::persist_state_change(
                            account_id,
                            previous_state,
                            CircuitState::HalfOpen,
                            Some("Timeout elapsed, testing recovery"),
                            None,
                        );

                        return None; // Allow this request through
                    }
                }
                circuit.last_failure_reason.clone()
            }
            // Closed and HalfOpen allow requests through
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
                // Check if we should transition to half-open
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

                        // Persist transition to half-open
                        Self::persist_state_change(
                            account_id,
                            previous_state,
                            CircuitState::HalfOpen,
                            Some("Timeout elapsed, testing recovery"),
                            None,
                        );

                        return Ok(()); // Allow this request through
                    }
                    // Return remaining time until half-open
                    let remaining = self.config.open_duration.saturating_sub(elapsed);
                    return Err(remaining);
                }
                // If opened_at is None (shouldn't happen), use config duration
                Err(self.config.open_duration)
            }
            // Closed and HalfOpen allow requests through
            CircuitState::Closed | CircuitState::HalfOpen => Ok(()),
        }
    }

    /// Record a successful request for an account
    pub fn record_success(&self, account_id: &str) {
        let mut circuits = self.circuits.write();
        let circuit = circuits.entry(account_id.to_string()).or_default();

        match circuit.state {
            CircuitState::Closed => {
                // Reset any failure count
                circuit.consecutive_failures = 0;
            }
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

                    // Persist recovery to database
                    Self::persist_state_change(
                        account_id,
                        previous_state,
                        CircuitState::Closed,
                        Some("Account recovered"),
                        None,
                    );
                }
            }
            CircuitState::Open => {
                // Shouldn't happen - success in open state
                debug!(
                    account_id = %account_id,
                    "Unexpected success in open state"
                );
            }
        }
    }

    /// Record a failed request for an account
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
                    circuit.opened_at = Some(Instant::now());
                    self.total_trips.fetch_add(1, Ordering::Relaxed);

                    // Persist state change to database
                    Self::persist_state_change(
                        account_id,
                        previous_state,
                        CircuitState::Open,
                        Some(reason),
                        Some(i32::try_from(circuit.consecutive_failures).unwrap_or(i32::MAX)),
                    );
                }
            }
            CircuitState::HalfOpen => {
                // Failure in half-open state - reopen the circuit
                warn!(
                    account_id = %account_id,
                    reason = %reason,
                    "Circuit breaker re-opening - failure during half-open"
                );
                circuit.state = CircuitState::Open;
                circuit.opened_at = Some(Instant::now());
                self.total_trips.fetch_add(1, Ordering::Relaxed);

                // Persist state change to database
                Self::persist_state_change(
                    account_id,
                    previous_state,
                    CircuitState::Open,
                    Some(reason),
                    Some(i32::try_from(circuit.consecutive_failures).unwrap_or(i32::MAX)),
                );
            }
            CircuitState::Open => {
                // Already open, just update failure count
            }
        }
    }

    /// Persist circuit breaker state change (logging only, DB persistence removed)
    fn persist_state_change(
        account_id: &str,
        previous_state: CircuitState,
        new_state: CircuitState,
        reason: Option<&str>,
        failure_count: Option<i32>,
    ) {
        let _ = failure_count; // Silence unused warning
        tracing::info!(
            "Circuit breaker state change: {} {:?} -> {:?} (reason: {:?})",
            account_id,
            previous_state,
            new_state,
            reason
        );
    }

    /// Get the current state of an account's circuit
    pub fn get_state(&self, account_id: &str) -> CircuitState {
        let circuits = self.circuits.read();
        circuits
            .get(account_id)
            .map_or(CircuitState::Closed, |c| c.state)
    }

    /// Get total number of circuit trips (for monitoring)
    pub fn total_trips(&self) -> u64 {
        self.total_trips.load(Ordering::Relaxed)
    }

    /// Reset a circuit for an account (e.g., after manual intervention)
    pub fn reset(&self, account_id: &str) {
        let mut circuits = self.circuits.write();
        if let Some(circuit) = circuits.get_mut(account_id) {
            let previous_state = circuit.state;
            info!(
                account_id = %account_id,
                previous_state = ?previous_state,
                "Circuit breaker reset manually"
            );

            // Persist manual reset to database
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

    /// Get summary of all circuit states
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

        CircuitBreakerSummary {
            closed,
            open,
            half_open,
            total_trips: self.total_trips(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            open_duration: Duration::from_secs(60),
            success_threshold: 2,
        };
        let manager = CircuitBreakerManager::with_config(config);

        // Initially closed
        assert!(manager.check("acc1").is_none());
        assert_eq!(manager.get_state("acc1"), CircuitState::Closed);

        // Record failures
        manager.record_failure("acc1", "error 1");
        manager.record_failure("acc1", "error 2");
        assert!(manager.check("acc1").is_none()); // Still closed

        manager.record_failure("acc1", "error 3");
        // Now should be open
        assert!(manager.check("acc1").is_some());
        assert_eq!(manager.get_state("acc1"), CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_success_resets_failures() {
        let manager = CircuitBreakerManager::default();

        manager.record_failure("acc1", "error");
        manager.record_failure("acc1", "error");
        manager.record_success("acc1");

        // Failures should be reset, circuit stays closed
        assert!(manager.check("acc1").is_none());
        assert_eq!(manager.get_state("acc1"), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_half_open_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            open_duration: Duration::from_millis(10), // Very short for testing
            success_threshold: 2,
        };
        let manager = CircuitBreakerManager::with_config(config);

        // Open the circuit
        manager.record_failure("acc1", "error");
        manager.record_failure("acc1", "error");
        assert_eq!(manager.get_state("acc1"), CircuitState::Open);

        // Wait for open duration
        std::thread::sleep(Duration::from_millis(15));

        // Check should transition to half-open and allow request
        assert!(manager.check("acc1").is_none());
        assert_eq!(manager.get_state("acc1"), CircuitState::HalfOpen);

        // Record successes to close
        manager.record_success("acc1");
        manager.record_success("acc1");
        assert_eq!(manager.get_state("acc1"), CircuitState::Closed);
    }
}
