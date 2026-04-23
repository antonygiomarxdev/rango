use std::sync::atomic::{AtomicU64, Ordering};

/// Lightweight operation counters for internal metrics.
#[derive(Debug, Default)]
pub struct Metrics {
    inserts: AtomicU64,
    finds: AtomicU64,
    updates: AtomicU64,
    deletes: AtomicU64,
    sync_pushes: AtomicU64,
    sync_pulls: AtomicU64,
}

impl Metrics {
    pub fn record_insert(&self) {
        self.inserts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_find(&self) {
        self.finds.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_update(&self) {
        self.updates.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_delete(&self) {
        self.deletes.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_sync_push(&self, n: usize) {
        self.sync_pushes.fetch_add(n as u64, Ordering::Relaxed);
    }

    pub fn record_sync_pull(&self, n: usize) {
        self.sync_pulls.fetch_add(n as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            inserts: self.inserts.load(Ordering::Relaxed),
            finds: self.finds.load(Ordering::Relaxed),
            updates: self.updates.load(Ordering::Relaxed),
            deletes: self.deletes.load(Ordering::Relaxed),
            sync_pushes: self.sync_pushes.load(Ordering::Relaxed),
            sync_pulls: self.sync_pulls.load(Ordering::Relaxed),
        }
    }
}

/// Serializable snapshot of metrics counters.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetricsSnapshot {
    pub inserts: u64,
    pub finds: u64,
    pub updates: u64,
    pub deletes: u64,
    pub sync_pushes: u64,
    pub sync_pulls: u64,
}
