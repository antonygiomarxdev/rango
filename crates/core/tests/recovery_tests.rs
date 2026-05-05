use std::sync::Arc;

use bson::doc;
use proptest::prelude::*;
use rango_core::RangoEngine;
use rango_oplog::{FileOplog, NullOplog};
use rango_storage::{MemoryStorage, RedbStorage};
use rango_types::{CollectionName, DocumentId, Mutation, MutationOp, Revision};

fn setup_memory(node_id: &str) -> RangoEngine {
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
        write_id: format!("recovery-replay-{value}"),
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
fn restore_from_persistent_workspace_recovers_state_after_simulated_crash() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let storage_path = tmpdir.path().join("storage");
    let oplog_path = tmpdir.path().join("oplog");

    let collection = CollectionName::new("recovery-persistent");
    let tenant_id = "tenant-a";
    let snapshot_id = "snap-recovery-1";

    // Phase 1: Insert 10 records, take snapshot, then simulate crash.
    let doc_ids: Vec<DocumentId>;
    {
        let storage = Arc::new(RedbStorage::open(&storage_path).unwrap());
        let oplog = Arc::new(FileOplog::new(&oplog_path).unwrap());
        let engine = RangoEngine::open(storage, oplog, "node-a").unwrap();

        // Insert 10 documents
        let mut ids = Vec::new();
        for i in 1..=10 {
            let id = engine
                .insert_one(&collection, doc! { "counter": i as i64 })
                .unwrap();
            ids.push(id);
        }
        doc_ids = ids;

        // Snapshot at base_seq = 10
        let _snapshot = engine
            .create_snapshot(&collection, tenant_id, snapshot_id, 10)
            .unwrap();

        // Simulate crash by dropping engine
        drop(engine);
    }

    // Phase 2: Re-open engine and restore from snapshot.
    {
        let storage = Arc::new(RedbStorage::open(&storage_path).unwrap());
        let oplog = Arc::new(FileOplog::new(&oplog_path).unwrap());
        let engine = RangoEngine::open(storage, oplog, "node-a").unwrap();

        // Verify pre-crash state is restored (RedbStorage persists data)
        let recovered_docs = engine.find_all_raw(&collection).unwrap();
        assert_eq!(
            recovered_docs.len(),
            10,
            "recovered state must contain all 10 persisted documents"
        );

        // Create a snapshot from recovered state
        let snapshot = rango_types::SnapshotUnit {
            snapshot_id: snapshot_id.to_string(),
            tenant_id: tenant_id.to_string(),
            namespace: collection.0.clone(),
            base_seq: 10,
            created_at: bson::DateTime::now(),
            state: recovered_docs,
        };

        // Apply bounded replay (post-snapshot mutations)
        let mut replay = Vec::new();
        for (idx, id) in doc_ids.iter().enumerate() {
            let value = (11 + idx) as i64;
            replay.push(replay_mutation_for(&collection, id, value));
        }

        let applied = engine
            .restore_from_snapshot(&collection, &snapshot, replay)
            .unwrap();

        // Assert mutations were applied
        assert!(
            applied > 0,
            "restore + replay must apply post-snapshot mutations"
        );

        // Verify final state is recovered
        let final_state = engine.find_all_raw(&collection).unwrap();
        assert_eq!(
            final_state.len(),
            10,
            "final state must contain all 10 documents"
        );
    }
}

#[test]
fn replay_after_snapshot_is_bounded_by_checkpoint() {
    let collection = CollectionName::new("recovery-bounded");
    let engine = setup_memory("node-a");

    // Insert 50 documents
    let mut ids = Vec::new();
    for i in 1..=50 {
        let id = engine
            .insert_one(&collection, doc! { "counter": i as i64 })
            .unwrap();
        ids.push((id, i as i64));
    }

    // Snapshot at seq 25
    let snapshot = engine
        .create_snapshot(&collection, "tenant-a", "snap-bounded", 25)
        .unwrap();
    assert_eq!(snapshot.base_seq, 25);

    // Prepare bounded replay: only mutations after seq 25 (seqs 26..=50, 25 mutations)
    let mut replay = Vec::new();
    for (id, seq) in ids.iter().skip(25) {
        replay.push(replay_mutation_for(&collection, id, *seq));
    }
    let expected_replay_count = replay.len();

    // Restore from snapshot and apply bounded replay
    let applied = engine
        .restore_from_snapshot(&collection, &snapshot, replay)
        .unwrap();

    // Assert the correct number of mutations were applied
    assert_eq!(
        applied, expected_replay_count,
        "bounded replay must apply exactly the mutations provided"
    );

    // Verify final state reflects all 50 documents
    let final_docs = engine.find_all_raw(&collection).unwrap();
    assert_eq!(
        final_docs.len(),
        50,
        "final state must contain all 50 documents after replay"
    );
}

proptest! {
    #[test]
    fn rollback_then_replay_yields_deterministic_convergence(
        mutation_count in 2usize..=15
    ) {
        let collection = CollectionName::new("recovery-proptest");

        // Pre-generate unique document IDs and counter values to ensure determinism
        let mut doc_ids = Vec::new();
        let mut counters = Vec::new();
        for i in 0..mutation_count {
            doc_ids.push(DocumentId::new_uuid_v7());
            counters.push((i as i64 + 1) * 10);
        }

        // Engine A: apply all mutations in sequence
        let engine_a = setup_memory("node-a");
        for (idx, &counter) in counters.iter().enumerate() {
            let _ = engine_a
                .insert_one(&collection, doc! { "_id": doc_ids[idx].0.clone(), "counter": counter })
                .unwrap();
        }

        // Create snapshot at midpoint
        let midpoint = (mutation_count / 2) as u64;
        let _snapshot_a = engine_a
            .create_snapshot(&collection, "tenant-a", "snap-a", midpoint)
            .unwrap();

        // Engine B: apply mutations up to midpoint, then snapshot
        let engine_b = setup_memory("node-b");
        for (idx, &counter) in counters.iter().enumerate().take(midpoint as usize) {
            let _ = engine_b
                .insert_one(&collection, doc! { "_id": doc_ids[idx].0.clone(), "counter": counter })
                .unwrap();
        }

        let snapshot_b = engine_b
            .create_snapshot(&collection, "tenant-a", "snap-b", midpoint)
            .unwrap();

        // Replay remainder on Engine B using same IDs from first batch
        let mut replay = Vec::new();
        for (idx, &counter) in counters.iter().enumerate().skip(midpoint as usize) {
            replay.push(replay_mutation_for(&collection, &doc_ids[idx], counter));
        }

        engine_b
            .restore_from_snapshot(&collection, &snapshot_b, replay)
            .unwrap();

        // Verify both engines end up with same document count (deterministic convergence)
        let docs_a = engine_a.find_all_raw(&collection).unwrap();
        let docs_b = engine_b.find_all_raw(&collection).unwrap();

        prop_assert_eq!(
            docs_a.len(),
            docs_b.len(),
            "engines must converge to same document count after snapshot + replay"
        );
    }
}
