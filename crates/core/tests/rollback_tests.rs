use std::sync::Arc;

use bson::doc;
use rango_core::RangoEngine;
use rango_oplog::NullOplog;
use rango_storage::MemoryStorage;
use rango_types::{CollectionName, DocumentId, Mutation, MutationOp, PolicyDecision, Revision};

fn setup(node_id: &str) -> RangoEngine<MemoryStorage> {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    RangoEngine::open(storage, oplog, node_id).unwrap()
}

fn replay_mutation_for(collection: &CollectionName, id: &DocumentId, value: i64) -> Mutation {
    let rev = Revision::now("node-a");
    Mutation {
        op: MutationOp::Update,
        collection: collection.0.clone(),
        doc_id: id.clone(),
        patch: Some(doc! {
            "_id": id.0.clone(),
            "_rev": rev.to_string(),
            "counter": value
        }),
        seq: value as u64,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: format!("rollback-replay-{value}"),
        metadata: rango_types::MutationMetadata {
            id: id.clone(),
            namespace: collection.0.clone(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-a".to_string(),
            actor: "node-a".to_string(),
            lineage: id.to_string(),
            schema_version: 1,
            trust_score: 1.0,
            verified: Some(true),
            expires_at: None,
        },
    }
}

#[test]
fn rollback_restores_valid_snapshot_and_replay_is_deterministic() {
    let collection = CollectionName::new("rollback");
    let engine = setup("node-a");
    let id = engine
        .insert_one(&collection, doc! { "counter": 1_i64 })
        .unwrap();

    let snapshot = engine
        .create_snapshot(&collection, "tenant-a", "snap-rollback-1", 1)
        .unwrap();

    engine
        .update_one(&collection, &id, doc! { "$set": { "counter": 2_i64 } })
        .unwrap();
    engine
        .update_one(&collection, &id, doc! { "$set": { "counter": 3_i64 } })
        .unwrap();

    let replay = vec![
        replay_mutation_for(&collection, &id, 4),
        replay_mutation_for(&collection, &id, 5),
    ];
    let applied = engine
        .restore_from_snapshot(&collection, &snapshot, replay)
        .unwrap();

    assert_eq!(applied, 2, "rollback replay must apply the exact replay window");
    let restored = engine.find_one(&collection, &id).unwrap().unwrap().data;
    assert_eq!(restored.get_i64("counter").unwrap(), 5);
}

#[test]
fn rollback_rejects_namespace_mismatch() {
    let source = setup("node-a");
    let target = setup("node-a");
    let source_coll = CollectionName::new("source");
    let target_coll = CollectionName::new("target");

    let _ = source
        .insert_one(&source_coll, doc! { "value": "snapshot-doc" })
        .unwrap();
    let snapshot = source
        .create_snapshot(&source_coll, "tenant-a", "snap-wrong-ns", 1)
        .unwrap();

    let result = target.restore_from_snapshot(&target_coll, &snapshot, vec![]);
    assert!(result.is_err(), "mismatched namespace must be rejected");
}

#[test]
fn rollback_must_emit_governance_visible_decision_evidence() {
    let collection = CollectionName::new("rollback-audit");
    let engine = setup("node-a");
    let id = engine
        .insert_one(&collection, doc! { "counter": 10_i64 })
        .unwrap();
    let snapshot = engine
        .create_snapshot(&collection, "tenant-a", "snap-audit", 1)
        .unwrap();

    let replay = vec![replay_mutation_for(&collection, &id, 11)];
    engine
        .restore_from_snapshot(&collection, &snapshot, replay)
        .unwrap();

    // RED expectation for Wave 0: rollback should surface explicit policy/audit outcome.
    let synthetic_rollback_decision = None::<PolicyDecision>;
    assert_eq!(
        synthetic_rollback_decision,
        Some(PolicyDecision::Allow),
        "rollback must emit explicit auditable allow/sanitize/reject decision",
    );
}

