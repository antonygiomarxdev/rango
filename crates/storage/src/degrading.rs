use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bson::Document;
use rango_types::{CollectionName, DocumentId, RangoError};
use tracing::{info, warn};

use crate::{RangeIter, ScanIter, StorageEngine};

/// Storage wrapper that degrades gracefully when disk space is low.
///
/// When available space drops below a threshold, writes are rejected
/// with a clear error while reads continue to work. This prevents
/// crashes and data corruption on storage exhaustion.
#[derive(Clone)]
pub struct DegradingStorage<S: StorageEngine> {
    inner: Arc<S>,
    /// Threshold in bytes below which writes are rejected
    min_free_bytes: u64,
    /// Current degraded state
    degraded: Arc<AtomicBool>,
    /// Last checked available space
    last_available: Arc<Mutex<u64>>,
    /// Check interval: only re-check space every N writes
    write_counter: Arc<Mutex<u64>>,
    check_interval: u64,
    /// Optional override for space checking (used in tests)
    space_checker: Arc<dyn Fn() -> Result<u64, RangoError> + Send + Sync>,
}

impl<S: StorageEngine> std::fmt::Debug for DegradingStorage<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DegradingStorage")
            .field("min_free_bytes", &self.min_free_bytes)
            .field("degraded", &self.is_degraded())
            .field("last_available", &self.last_available_space())
            .finish()
    }
}

impl<S: StorageEngine> DegradingStorage<S> {
    /// Wrap an existing storage engine with degradation monitoring.
    pub fn new(
        inner: S,
        path: impl AsRef<Path>,
        min_free_bytes: u64,
    ) -> Result<Self, RangoError> {
        let path = path.as_ref().to_path_buf();
        let space_checker = Arc::new(move || {
            fs2::available_space(&path)
                .map_err(|e| RangoError::Storage(format!("Failed to check disk space: {e}")))
        });
        let available = space_checker()?;
        info!(
            available_mb = available / 1024 / 1024,
            threshold_mb = min_free_bytes / 1024 / 1024,
            "DegradingStorage initialized"
        );
        Ok(Self {
            inner: Arc::new(inner),
            min_free_bytes,
            degraded: Arc::new(AtomicBool::new(false)),
            last_available: Arc::new(Mutex::new(available)),
            write_counter: Arc::new(Mutex::new(0)),
            check_interval: 10,
            space_checker,
        })
    }

    /// Create with sensible defaults: 100MB minimum free space.
    pub fn with_default_threshold(
        inner: S,
        path: impl AsRef<Path>,
    ) -> Result<Self, RangoError> {
        Self::new(inner, path, 100 * 1024 * 1024)
    }

    /// Check if currently in degraded mode.
    pub fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::Relaxed)
    }

    /// Current available space (last checked).
    pub fn last_available_space(&self) -> u64 {
        *self.last_available.lock().unwrap()
    }

    fn maybe_check_and_degrade(&self) -> Result<(), RangoError> {
        let should_check = {
            let mut counter = self.write_counter.lock().unwrap();
            *counter += 1;
            *counter >= self.check_interval
        };

        if should_check {
            *self.write_counter.lock().unwrap() = 0;
            match (self.space_checker)() {
                Ok(available) => {
                    *self.last_available.lock().unwrap() = available;
                    let was_degraded = self.degraded.load(Ordering::Relaxed);
                    let now_degraded = available < self.min_free_bytes;

                    if now_degraded && !was_degraded {
                        warn!(
                            available_mb = available / 1024 / 1024,
                            threshold_mb = self.min_free_bytes / 1024 / 1024,
                            "Storage degraded: entering read-only mode"
                        );
                        self.degraded.store(true, Ordering::Relaxed);
                    } else if !now_degraded && was_degraded {
                        info!(
                            available_mb = available / 1024 / 1024,
                            "Storage recovered: resuming writes"
                        );
                        self.degraded.store(false, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    // If we can't check space, err on the side of caution
                    warn!("Failed to check disk space, assuming degraded: {e}");
                    self.degraded.store(true, Ordering::Relaxed);
                }
            }
        }

        if self.degraded.load(Ordering::Relaxed) {
            return Err(RangoError::Storage(
                "Storage degraded: insufficient disk space".to_string(),
            ));
        }

        Ok(())
    }
}

impl<S: StorageEngine> StorageEngine for DegradingStorage<S> {
    fn get(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
    ) -> Result<Option<Document>, RangoError> {
        self.inner.get(collection, id)
    }

    fn put(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
        doc: &Document,
    ) -> Result<(), RangoError> {
        self.maybe_check_and_degrade()?;
        self.inner.put(collection, id, doc)
    }

    fn delete(&self, collection: &CollectionName, id: &DocumentId) -> Result<bool, RangoError> {
        self.maybe_check_and_degrade()?;
        self.inner.delete(collection, id)
    }

    fn scan(&self, collection: &CollectionName) -> Result<Box<ScanIter>, RangoError> {
        self.inner.scan(collection)
    }

    fn range(
        &self,
        collection: &CollectionName,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<RangeIter>, RangoError> {
        self.inner.range(collection, start, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryStorage;
    use bson::doc;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Create a DegradingStorage with a custom space checker for testing.
    fn with_checker<S: StorageEngine>(
        inner: S,
        min_free: u64,
        checker: impl Fn() -> Result<u64, RangoError> + Send + Sync + 'static,
    ) -> DegradingStorage<S> {
        DegradingStorage {
            inner: Arc::new(inner),
            min_free_bytes: min_free,
            degraded: Arc::new(AtomicBool::new(false)),
            last_available: Arc::new(Mutex::new(u64::MAX)),
            write_counter: Arc::new(Mutex::new(0)),
            check_interval: 1, // check every write for determinism
            space_checker: Arc::new(checker),
        }
    }

    #[test]
    fn degrading_storage_passes_through_reads() {
        let inner = MemoryStorage::new();
        let storage = with_checker(inner, 100, || Ok(u64::MAX));
        let collection = CollectionName::new("users");
        let id = DocumentId::new_uuid_v7();

        // Write should work initially
        storage.put(&collection, &id, &doc! { "name": "alice" }).unwrap();

        // Read should always work
        let found = storage.get(&collection, &id).unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn degrading_storage_rejects_writes_when_space_low() {
        let inner = MemoryStorage::new();
        let available = Arc::new(AtomicU64::new(1000));
        let a2 = available.clone();
        let storage = with_checker(inner, 100, move || Ok(a2.load(Ordering::Relaxed)));
        let collection = CollectionName::new("users");
        let id = DocumentId::new_uuid_v7();

        // Write should work with plenty of space
        available.store(1000, Ordering::Relaxed);
        storage.put(&collection, &id, &doc! { "name": "alice" }).unwrap();
        assert!(!storage.is_degraded());

        // Degrade by dropping available space below threshold
        available.store(50, Ordering::Relaxed);
        let result = storage.put(&collection, &id, &doc! { "name": "bob" });
        assert!(result.is_err(), "should reject writes when space is below threshold");
        assert!(storage.is_degraded());

        // Reads still work even when degraded
        let found = storage.get(&collection, &id).unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn degrading_storage_recovers_when_space_frees() {
        let inner = MemoryStorage::new();
        let available = Arc::new(AtomicU64::new(50));
        let a2 = available.clone();
        let storage = with_checker(inner, 100, move || Ok(a2.load(Ordering::Relaxed)));
        let collection = CollectionName::new("users");
        let id = DocumentId::new_uuid_v7();

        // Start degraded
        let result = storage.put(&collection, &id, &doc! { "name": "alice" });
        assert!(result.is_err());
        assert!(storage.is_degraded());

        // Recover by increasing available space
        available.store(200, Ordering::Relaxed);
        storage.put(&collection, &id, &doc! { "name": "alice" }).unwrap();
        assert!(!storage.is_degraded());
    }

    #[test]
    fn degrading_storage_delete_blocked_when_degraded() {
        let inner = MemoryStorage::new();
        let storage = with_checker(inner, 100, || Ok(50));
        let collection = CollectionName::new("users");
        let id = DocumentId::new_uuid_v7();

        let result = storage.delete(&collection, &id);
        assert!(result.is_err(), "delete should be blocked when degraded");
    }
}
