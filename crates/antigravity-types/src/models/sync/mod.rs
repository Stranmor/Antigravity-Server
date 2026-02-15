//! Distributed Sync Types for Model Routing
//!
//! Implements Last-Write-Wins (LWW) merge strategy for bidirectional
//! synchronization of model mappings between instances.
//!
//! # Architecture
//!
//! Uses the same strategy as AWS DynamoDB and Apache Cassandra:
//! - Each entry has a timestamp (unix millis)
//! - On merge, newer timestamp wins per key
//! - No conflicts, eventual consistency guaranteed

mod mapping;
mod proxy_assignment;

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    reason = "test assertions"
)]
mod tests;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    reason = "test assertions"
)]
mod tests_proxy_assignment;
#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    reason = "test assertions"
)]
mod tests_tiebreaker;

pub use mapping::{MappingEntry, SyncableMapping};
pub use proxy_assignment::{ProxyAssignment, SyncableProxyAssignments};
