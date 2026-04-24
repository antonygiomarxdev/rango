use std::sync::Arc;

use bson::doc;
use rango_core::RangoEngine;
use rango_oplog::NullOplog;
use rango_storage::MemoryStorage;
use rango_types::{CollectionName, DocumentId};

fn setup() -> RangoEngine<MemoryStorage> {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    RangoEngine::open(storage, oplog, "test-node").unwrap()
}

#[test]
fn test_insert_and_find_one() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = engine
        .insert_one(&coll, doc! { "name": "Alice", "age": 30 })
        .unwrap();
    let found = engine.find_one(&coll, &id).unwrap();

    assert!(found.is_some());
    let doc = found.unwrap();
    assert_eq!(doc.data.get_str("name").unwrap(), "Alice");
    assert_eq!(doc.data.get_i32("age").unwrap(), 30);
}

#[test]
fn test_insert_generates_uuid_v7() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = engine.insert_one(&coll, doc! { "name": "Bob" }).unwrap();

    // UUID v7 is a BSON Binary with Uuid subtype
    match &id.0 {
        bson::Bson::Binary(bin) => {
            assert_eq!(bin.subtype, bson::spec::BinarySubtype::Uuid);
        }
        other => panic!("Expected Binary UUID, got {:?}", other),
    }
}

#[test]
fn test_insert_preserves_existing_id() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let existing_id = bson::oid::ObjectId::new();
    let id = engine
        .insert_one(&coll, doc! { "_id": existing_id, "name": "Charlie" })
        .unwrap();

    match &id.0 {
        bson::Bson::ObjectId(oid) => {
            assert_eq!(*oid, existing_id);
        }
        other => panic!("Expected ObjectId, got {:?}", other),
    }
}

#[test]
fn test_find_one_not_found() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = DocumentId::new_uuid_v7();
    let found = engine.find_one(&coll, &id).unwrap();

    assert!(found.is_none());
}

#[test]
fn test_find_many() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    engine.insert_one(&coll, doc! { "name": "Bob" }).unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Charlie" })
        .unwrap();

    let cursor = engine.find_many(&coll).unwrap();
    let mut count = 0;
    for _doc in cursor {
        count += 1;
    }

    assert_eq!(count, 3);
}

#[test]
fn test_update_one_set() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = engine
        .insert_one(&coll, doc! { "name": "Alice", "age": 30 })
        .unwrap();
    let updated = engine
        .update_one(&coll, &id, doc! { "$set": { "age": 31 } })
        .unwrap();

    assert!(updated);
    let found = engine.find_one(&coll, &id).unwrap().unwrap();
    assert_eq!(found.data.get_i32("age").unwrap(), 31);
    assert_eq!(found.data.get_str("name").unwrap(), "Alice"); // name unchanged
}

#[test]
fn test_update_one_unset() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = engine
        .insert_one(&coll, doc! { "name": "Alice", "age": 30 })
        .unwrap();
    let updated = engine
        .update_one(&coll, &id, doc! { "$unset": { "age": "" } })
        .unwrap();

    assert!(updated);
    let found = engine.find_one(&coll, &id).unwrap().unwrap();
    assert!(!found.data.contains_key("age"));
}

#[test]
fn test_update_one_inc() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = engine
        .insert_one(&coll, doc! { "name": "Alice", "score": 100i64 })
        .unwrap();
    let updated = engine
        .update_one(&coll, &id, doc! { "$inc": { "score": 5i64 } })
        .unwrap();

    assert!(updated);
    let found = engine.find_one(&coll, &id).unwrap().unwrap();
    assert_eq!(found.data.get_i64("score").unwrap(), 105);
}

#[test]
fn test_update_one_not_found() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = DocumentId::new_uuid_v7();
    let updated = engine
        .update_one(&coll, &id, doc! { "$set": { "x": 1 } })
        .unwrap();

    assert!(!updated);
}

#[test]
fn test_delete_one() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    let deleted = engine.delete_one(&coll, &id).unwrap();

    assert!(deleted);
    let found = engine.find_one(&coll, &id).unwrap();
    assert!(found.is_none());

    // Verify tombstone exists in raw storage
    let raw = engine.find_all_raw(&coll).unwrap();
    assert_eq!(raw.len(), 1);
    assert!(raw[0].get_bool("_deleted").unwrap());
}

#[test]
fn test_delete_one_not_found() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = DocumentId::new_uuid_v7();
    let deleted = engine.delete_one(&coll, &id).unwrap();

    assert!(!deleted);
}

#[test]
fn test_revision_set_on_insert() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    let found = engine.find_one(&coll, &id).unwrap().unwrap();
    let rev = found.data.get_str("_rev").unwrap();

    // HLC format: <timestamp>-<counter>-<node_hash>
    assert!(!rev.is_empty());
    assert!(rev.contains('-'));
}

#[test]
fn test_updated_at_set_on_insert() {
    let engine = setup();
    let coll = CollectionName::new("test");

    let id = engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    let found = engine.find_one(&coll, &id).unwrap().unwrap();

    assert!(found.data.contains_key("_updated_at"));
}

#[test]
fn test_document_size_limit() {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    let config = rango_types::RangoConfig {
        max_document_size_bytes: 300,
        memory_budget_bytes: 1024 * 1024,
    };
    let engine = RangoEngine::open_with_config(storage, oplog, "test-node", config).unwrap();
    let coll = CollectionName::new("test");

    // Small document should succeed (with metadata, stays under 300 bytes)
    let id = engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    assert!(engine.find_one(&coll, &id).unwrap().is_some());

    // Large document should fail
    let big_value = "x".repeat(500);
    let result = engine.insert_one(&coll, doc! { "data": big_value });
    assert!(result.is_err());
    match result {
        Err(rango_types::RangoError::DocumentTooLarge { size, limit }) => {
            assert!(size > 300);
            assert_eq!(limit, 300);
        }
        other => panic!("Expected DocumentTooLarge error, got {:?}", other),
    }
}

#[test]
fn test_large_collection_streaming() {
    let engine = setup();
    let coll = CollectionName::new("bulk");

    // Insert 10_000 small documents
    for i in 0..10_000 {
        engine
            .insert_one(&coll, doc! { "index": i as i64 })
            .unwrap();
    }

    // Stream via find_many (no sort = true streaming)
    let cursor = engine.find_many(&coll).unwrap();
    let mut count = 0;
    for result in cursor {
        let _doc = result.unwrap();
        count += 1;
    }
    assert_eq!(count, 10_000);

    // Stream with filter and limit (should still be lazy)
    let cursor = engine
        .find(
            &coll,
            &doc! { "index": { "$gte": 5000i64 } },
            None,
            None,
            None,
            Some(100),
        )
        .unwrap();
    let count = cursor.count();
    assert_eq!(count, 100);
}
