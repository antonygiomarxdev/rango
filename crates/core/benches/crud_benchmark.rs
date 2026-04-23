use bson::doc;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rango_core::RangoEngine;
use rango_oplog::NullOplog;
use rango_storage::MemoryStorage;
use rango_types::CollectionName;
use std::sync::Arc;

fn bench_insert_one(c: &mut Criterion) {
    c.bench_function("insert_one", |b| {
        let storage = Arc::new(MemoryStorage::new());
        let oplog = Arc::new(NullOplog::new());
        let engine = RangoEngine::open(storage, oplog, "bench").unwrap();
        let coll = CollectionName::new("bench");
        let mut i = 0u64;

        b.iter(|| {
            i += 1;
            engine
                .insert_one(
                    &coll,
                    doc! {
                        "index": i as i64,
                        "name": format!("user-{}", i),
                    },
                )
                .unwrap();
        });
    });
}

fn bench_find_one_by_id(c: &mut Criterion) {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    let engine = RangoEngine::open(storage, oplog, "bench").unwrap();
    let coll = CollectionName::new("bench");

    let id = engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();

    c.bench_function("find_one_by_id", |b| {
        b.iter(|| {
            black_box(engine.find_one(&coll, &id).unwrap());
        });
    });
}

fn bench_find_with_filter(c: &mut Criterion) {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    let engine = RangoEngine::open(storage, oplog, "bench").unwrap();
    let coll = CollectionName::new("bench");

    for i in 0..1000 {
        engine
            .insert_one(
                &coll,
                doc! {
                    "index": i as i64,
                    "name": format!("user-{}", i),
                },
            )
            .unwrap();
    }

    c.bench_function("find_with_filter", |b| {
        b.iter(|| {
            let mut cursor = engine
                .find(
                    &coll,
                    &doc! { "index": { "$gte": 500i64 } },
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
            black_box(cursor.count());
        });
    });
}

fn bench_update_one(c: &mut Criterion) {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    let engine = RangoEngine::open(storage, oplog, "bench").unwrap();
    let coll = CollectionName::new("bench");

    let id = engine
        .insert_one(&coll, doc! { "name": "Alice", "age": 30i32 })
        .unwrap();

    c.bench_function("update_one", |b| {
        b.iter(|| {
            black_box(
                engine
                    .update_one(&coll, &id, doc! { "$set": { "age": 31i32 } })
                    .unwrap(),
            );
        });
    });
}

criterion_group!(
    benches,
    bench_insert_one,
    bench_find_one_by_id,
    bench_find_with_filter,
    bench_update_one
);
criterion_main!(benches);
