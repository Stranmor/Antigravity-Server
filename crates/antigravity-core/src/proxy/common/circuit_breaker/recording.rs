use super::state::{AccountCircuit, CircuitBreakerSummary, CircuitState};
use super::CircuitBreakerManager;
use std::sync::atomic::Ordering;
use tracing::{debug, info, warn};

impl CircuitBreakerManager {
    pub fn record_success(&self, account_id: &str) {
        let mut circuits = self.circuits.write();
        let circuit = circuits.entry(account_id.to_string()).or_default();

        match circuit.state {
            CircuitState::Closed => {
                circuit.consecutive_failures = 0;
            },
            CircuitState::HalfOpen => {
                circuit.consecutive_successes += 1;
                circuit.half_open_probe_active = false;
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
                circuit.half_open_probe_active = false;
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

    pub(super) fn persist_state_change(
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
