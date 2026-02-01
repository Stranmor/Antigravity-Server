//! RAII guard for cancellation-safe active request counting.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// RAII guard for cancellation-safe increment/decrement of active_requests.
/// Decrements on drop unless `release()` is called.
pub struct ActiveRequestGuard {
    active_requests: Arc<DashMap<String, AtomicU32>>,
    key: String,
    released: bool,
}

impl ActiveRequestGuard {
    pub fn new(active_requests: Arc<DashMap<String, AtomicU32>>, key: String) -> Self {
        active_requests
            .entry(key.clone())
            .or_insert_with(|| AtomicU32::new(0))
            .fetch_add(1, Ordering::SeqCst);
        Self {
            active_requests,
            key,
            released: false,
        }
    }

    pub fn release(mut self) {
        self.released = true;
    }
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        if !self.released {
            if let Some(counter) = self.active_requests.get(&self.key) {
                let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                    if v > 0 {
                        Some(v - 1)
                    } else {
                        None
                    }
                });
            }
        }
    }
}
