use std::sync::Arc;

use bson::doc;
use rango_core::RangoEngine;
use rango_oplog::NullOplog;
use rango_storage::MemoryStorage;
use rango_types::{CollectionName, DocumentId, Revision};
use std::str::FromStr;

fn setup(node_id: &str) -> RangoEngine<MemoryStorage> {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    RangoEngine::open(storage, oplog, node_id).unwrap()
}

#[test]
fn test_reconciliation_lww() {
    let coll = CollectionName::new("test");
    
    // Node A creates doc
    let engine_a = setup("node-a");
    let id = engine_a.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    
    // Simulate network partition: both nodes get the doc
    let doc_a = engine_a.find_one(&coll, &id).unwrap().unwrap().data;
    
    // Node B applies the same doc (as if it received it via sync)
    let engine_b = setup("node-b");
    engine_b.apply_remote_mutation(&coll, doc_a.clone()).unwrap();
    
    // Both mutate concurrently
    engine_a.update_one(&coll, &id, doc! { "$set": { "name": "Alice Updated A" } }).unwrap();
    engine_b.update_one(&coll, &id, doc! { "$set": { "name": "Bob Updated B" } }).unwrap();
    
    // Get final docs
    let final_a = engine_a.find_one(&coll, &id).unwrap().unwrap().data;
    let final_b = engine_b.find_one(&coll, &id).unwrap().unwrap().data;
    
    // Now reconcile: B applies A's mutation, A applies B's mutation
    engine_b.apply_remote_mutation(&coll, final_a.clone()).unwrap();
    engine_a.apply_remote_mutation(&coll, final_b.clone()).unwrap();
    
    // Both should have the same winner (higher HLC wins)
    let reconciled_a = engine_a.find_one(&coll, &id).unwrap().unwrap().data;
    let reconciled_b = engine_b.find_one(&coll, &id).unwrap().unwrap().data;
    
    assert_eq!(
        reconciled_a.get_str("name").unwrap(),
        reconciled_b.get_str("name").unwrap()
    );
    
    // The loser should be in _conflicts
    let conflicts_a = engine_a.list_conflicts(&coll).unwrap();
    let conflicts_b = engine_b.list_conflicts(&coll).unwrap();
    
    // At least one node should have conflicts
    assert!(!conflicts_a.is_empty() || !conflicts_b.is_empty());
}

#[test]
fn test_idempotent_apply() {
    let engine = setup("node-a");
    let coll = CollectionName::new("test");
    
    let id = engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    let doc = engine.find_one(&coll, &id).unwrap().unwrap().data;
    
    // Apply same mutation 3 times
    engine.apply_remote_mutation(&coll, doc.clone()).unwrap();
    engine.apply_remote_mutation(&coll, doc.clone()).unwrap();
    engine.apply_remote_mutation(&coll, doc.clone()).unwrap();
    
    let final_doc = engine.find_one(&coll, &id).unwrap().unwrap().data;
    assert_eq!(final_doc.get_str("name").unwrap(), "Alice");
    
    // Conflicts should not grow indefinitely for identical revisions
    let conflicts = engine.list_conflicts(&coll).unwrap();
    // Same revision applied multiple times should be a no-op after first
    assert!(conflicts.is_empty() || conflicts[0].1.len() <= 1);
}

#[test]
fn test_conflict_resolution() {
    let engine = setup("node-a");
    let coll = CollectionName::new("test");
    
    let id = engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    let original = engine.find_one(&coll, &id).unwrap().unwrap().data;
    
    // Parse original revision and create a higher one
    let orig_rev_str = original.get_str("_rev").unwrap();
    let orig_rev = Revision::from_str(orig_rev_str).unwrap();
    let higher_rev = Revision::new(orig_rev.timestamp_ms() + 1, 0, "node-b");
    
    // Create a remote mutation with higher revision
    let mut remote = original.clone();
    remote.insert("name", "Remote Winner");
    remote.insert("_rev", higher_rev.to_string());
    
    engine.apply_remote_mutation(&coll, remote).unwrap();
    
    // Verify winner and conflict
    let doc = engine.find_one(&coll, &id).unwrap().unwrap().data;
    assert_eq!(doc.get_str("name").unwrap(), "Remote Winner");
    
    let conflicts = engine.list_conflicts(&coll).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].0, id);
    
    // Resolve conflict by choosing the original version
    let original_rev = Revision::from_str(orig_rev_str).unwrap();
    engine.resolve_conflict(&coll, &id, &original_rev).unwrap();
    
    let resolved = engine.find_one(&coll, &id).unwrap().unwrap().data;
    assert_eq!(resolved.get_str("name").unwrap(), "Alice");
    
    // Conflicts should be empty or reduced
    let remaining = engine.list_conflicts(&coll).unwrap();
    assert!(remaining.is_empty() || !remaining.iter().any(|(cid, _)| cid == &id));
}

#[test]
fn test_tombstone_sync() {
    let engine_a = setup("node-a");
    let coll = CollectionName::new("test");
    
    let id = engine_a.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    engine_a.delete_one(&coll, &id).unwrap();
    
    // Verify deleted doc is not findable by default
    assert!(engine_a.find_one(&coll, &id).unwrap().is_none());
    
    // But exists as tombstone in raw storage
    let raw = engine_a.find_all_raw(&coll).unwrap();
    assert_eq!(raw.len(), 1);
    assert!(raw[0].get_bool("_deleted").unwrap());
}
