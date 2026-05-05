use std::sync::Arc;

use bson::doc;
use rango_core::RangoEngine;
use rango_oplog::NullOplog;
use rango_storage::MemoryStorage;
use rango_types::CollectionName;

fn setup() -> RangoEngine {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    RangoEngine::open(storage, oplog, "test-node").unwrap()
}

#[test]
fn test_find_with_eq_filter() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine
        .insert_one(&coll, doc! { "name": "Alice", "age": 30 })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Bob", "age": 25 })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Charlie", "age": 35 })
        .unwrap();

    let cursor = engine
        .find(&coll, &doc! { "name": "Alice" }, None, None, None, None)
        .unwrap();
    let results: Vec<_> = cursor.filter_map(|r| r.ok()).collect();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].data.get_str("name").unwrap(), "Alice");
}

#[test]
fn test_find_with_gt_filter() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine
        .insert_one(&coll, doc! { "name": "Alice", "score": 80i64 })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Bob", "score": 95i64 })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Charlie", "score": 70i64 })
        .unwrap();

    let cursor = engine
        .find(
            &coll,
            &doc! { "score": { "$gt": 75i64 } },
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let results: Vec<_> = cursor.filter_map(|r| r.ok()).collect();

    assert_eq!(results.len(), 2);
}

#[test]
fn test_find_with_in_filter() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine
        .insert_one(&coll, doc! { "name": "Alice", "status": "active" })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Bob", "status": "pending" })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Charlie", "status": "inactive" })
        .unwrap();

    let cursor = engine
        .find(
            &coll,
            &doc! { "status": { "$in": ["active", "pending"] } },
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let results: Vec<_> = cursor.filter_map(|r| r.ok()).collect();

    assert_eq!(results.len(), 2);
}

#[test]
fn test_find_with_and_filter() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine
        .insert_one(&coll, doc! { "name": "Alice", "age": 30, "active": true })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Bob", "age": 25, "active": true })
        .unwrap();
    engine
        .insert_one(
            &coll,
            doc! { "name": "Charlie", "age": 30, "active": false },
        )
        .unwrap();

    let cursor = engine
        .find(
            &coll,
            &doc! { "$and": [{ "age": 30i64 }, { "active": true }] },
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let results: Vec<_> = cursor.filter_map(|r| r.ok()).collect();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].data.get_str("name").unwrap(), "Alice");
}

#[test]
fn test_find_with_or_filter() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine
        .insert_one(&coll, doc! { "name": "Alice", "role": "admin" })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Bob", "role": "user" })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Charlie", "role": "guest" })
        .unwrap();

    let cursor = engine
        .find(
            &coll,
            &doc! { "$or": [{ "role": "admin" }, { "role": "user" }] },
            None,
            None,
            None,
            None,
        )
        .unwrap();
    let results: Vec<_> = cursor.filter_map(|r| r.ok()).collect();

    assert_eq!(results.len(), 2);
}

#[test]
fn test_find_with_limit() {
    let engine = setup();
    let coll = CollectionName::new("test");

    for i in 0..10 {
        engine.insert_one(&coll, doc! { "index": i }).unwrap();
    }

    let cursor = engine
        .find(&coll, &doc! {}, None, None, None, Some(3))
        .unwrap();
    let results: Vec<_> = cursor.filter_map(|r| r.ok()).collect();

    assert_eq!(results.len(), 3);
}

#[test]
fn test_find_with_skip() {
    let engine = setup();
    let coll = CollectionName::new("test");

    for i in 0..5 {
        engine.insert_one(&coll, doc! { "index": i }).unwrap();
    }

    let cursor = engine
        .find(&coll, &doc! {}, None, None, Some(2), None)
        .unwrap();
    let results: Vec<_> = cursor.filter_map(|r| r.ok()).collect();

    assert_eq!(results.len(), 3); // 5 - 2 = 3
}

#[test]
fn test_find_with_projection_include() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine
        .insert_one(
            &coll,
            doc! { "name": "Alice", "age": 30, "email": "alice@example.com" },
        )
        .unwrap();

    let mut cursor = engine
        .find(&coll, &doc! {}, Some(&doc! { "name": 1 }), None, None, None)
        .unwrap();
    let doc = cursor.next().unwrap().unwrap();

    assert!(doc.data.contains_key("name"));
    assert!(!doc.data.contains_key("age"));
    assert!(doc.data.contains_key("_id")); // _id always included unless excluded
}

#[test]
fn test_find_with_projection_exclude() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine
        .insert_one(
            &coll,
            doc! { "name": "Alice", "age": 30, "secret": "password" },
        )
        .unwrap();

    let mut cursor = engine
        .find(
            &coll,
            &doc! {},
            Some(&doc! { "secret": 0 }),
            None,
            None,
            None,
        )
        .unwrap();
    let doc = cursor.next().unwrap().unwrap();

    assert!(doc.data.contains_key("name"));
    assert!(!doc.data.contains_key("secret"));
}

#[test]
fn test_find_with_sort() {
    let engine = setup();
    let coll = CollectionName::new("test");

    engine
        .insert_one(&coll, doc! { "name": "Charlie", "score": 30i64 })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Alice", "score": 10i64 })
        .unwrap();
    engine
        .insert_one(&coll, doc! { "name": "Bob", "score": 20i64 })
        .unwrap();

    let cursor = engine
        .find(&coll, &doc! {}, None, Some(("score", false)), None, None)
        .unwrap();
    let results: Vec<_> = cursor.filter_map(|r| r.ok()).collect();

    assert_eq!(results[0].data.get_str("name").unwrap(), "Alice");
    assert_eq!(results[1].data.get_str("name").unwrap(), "Bob");
    assert_eq!(results[2].data.get_str("name").unwrap(), "Charlie");
}
