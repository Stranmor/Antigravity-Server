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
mod tests;
#[cfg(test)]
mod tests_tiebreaker;

pub use mapping::{MappingEntry, SyncableMapping};
