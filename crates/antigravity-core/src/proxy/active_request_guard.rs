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
        Self { active_requests, key, released: false }
    }

    /// Atomically try to reserve a slot if current count < max_concurrent.
    /// Returns None if limit would be exceeded (no race condition).
    pub fn try_new(
        active_requests: Arc<DashMap<String, AtomicU32>>,
        key: String,
        max_concurrent: u32,
    ) -> Option<Self> {
        active_requests.entry(key.clone()).or_insert_with(|| AtomicU32::new(0));

        let counter_ref = active_requests.get(&key)?;
        loop {
            let current = counter_ref.load(Ordering::SeqCst);
            if current >= max_concurrent {
                return None;
            }
            if counter_ref
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                drop(counter_ref);
                return Some(Self { active_requests, key, released: false });
            }
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
