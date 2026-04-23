use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bson::doc;
use rango_core::RangoEngine;
use rango_oplog::Oplog;
use rango_storage::MemoryStorage;
use rango_types::{CollectionName, OplogEntry, RangoError};

#[derive(Default)]
struct InMemoryOplog {
    seq: AtomicU64,
    entries: Mutex<Vec<OplogEntry>>,
}

impl Oplog for InMemoryOplog {
    fn append(&self, mut entry: OplogEntry) -> Result<u64, RangoError> {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        entry.seq = seq;
        self.entries.lock().unwrap().push(entry);
        Ok(seq)
    }

    fn read_since(&self, seq: u64, limit: usize) -> Result<Vec<OplogEntry>, RangoError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.seq >= seq)
            .take(limit)
            .cloned()
            .collect())
    }

    fn mark_applied(&self, _seq: u64) -> Result<(), RangoError> {
        Ok(())
    }

    fn latest_seq(&self) -> Result<u64, RangoError> {
        Ok(self.seq.load(Ordering::Relaxed))
    }
}

#[test]
fn local_insert_update_delete_emit_non_empty_write_ids() {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(InMemoryOplog::default());
    let engine = RangoEngine::open(storage, oplog.clone(), "node-a").unwrap();
    let collection = CollectionName::new("local-write-id");

    let id = engine
        .insert_one(&collection, doc! { "name": "Alice" })
        .unwrap();
    engine
        .update_one(&collection, &id, doc! { "$set": { "name": "Alice 2" } })
        .unwrap();
    engine.delete_one(&collection, &id).unwrap();

    let entries = oplog.read_since(1, 10).unwrap();
    assert_eq!(entries.len(), 3);
    assert!(
        entries
            .iter()
            .all(|entry| !entry.mutation.write_id.trim().is_empty()),
        "all local mutations must carry non-empty write_id"
    );
}
