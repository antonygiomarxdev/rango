use std::sync::Arc;

use bson::doc;
use rango_core::RangoEngine;
use rango_oplog::NullOplog;
use rango_storage::MemoryStorage;
use rango_types::{CollectionName, MemoryTier};

fn setup(node_id: &str) -> RangoEngine {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    RangoEngine::open(storage, oplog, node_id).unwrap()
}

#[test]
fn invalidation_and_rebuild_do_not_mutate_canonical_truth() {
    let engine = setup("node-a");
    let canonical = CollectionName::new("state");
    let id = engine
        .insert_one(&canonical, doc! { "name": "Alice", "status": "gold" })
        .unwrap();

    let before = engine.find_one(&canonical, &id).unwrap().unwrap().data;

    engine
        .invalidate_semantic_projection(
            "tenant-a",
            "ns-a",
            &id.to_string(),
            "1000-0-node-a",
            "policy-v1",
        )
        .expect("invalidation should be accepted for derived tier");

    let rebuilt = engine
        .rebuild_semantic_projection(
            "tenant-a",
            "ns-a",
            &id.to_string(),
            "1000-0-node-a",
            MemoryTier::Semantic,
            doc! { "summary": "Alice is a trusted gold customer" },
        )
        .expect("rebuild should create a derived projection envelope");

    assert_eq!(rebuilt.metadata.tenant_id, "tenant-a");
    assert_eq!(rebuilt.metadata.namespace, "ns-a");
    assert_eq!(rebuilt.source_revision, "1000-0-node-a");
    assert_eq!(rebuilt.artifact_type, "semantic_projection");

    let after = engine.find_one(&canonical, &id).unwrap().unwrap().data;
    assert_eq!(after, before);
}
