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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::get_unwrap)]
mod tests;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::get_unwrap)]
mod tests_tiebreaker;

pub use mapping::{MappingEntry, SyncableMapping};
