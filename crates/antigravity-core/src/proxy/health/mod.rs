//! Account Health Monitoring Module
//!
//! Provides comprehensive health tracking for proxy accounts with:
//! - Consecutive error tracking per account
//! - Auto-disable on error threshold exceeded
//! - Automatic recovery after cooldown period
//! - State transition logging (enabled -> disabled -> enabled)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  HealthMonitor                                               │
//! │  ├── accounts: DashMap<String, AccountHealth>               │
//! │  ├── recovery_task: Background task for auto-recovery       │
//! │  └── config: HealthConfig                                    │
//! └─────────────────────────────────────────────────────────────┘
//! ```

mod monitor;
mod response;
mod types;

#[cfg(test)]
mod tests;

pub use monitor::HealthMonitor;
pub use types::{AccountHealth, AccountHealthResponse, ErrorType, HealthConfig, HealthStatus};
