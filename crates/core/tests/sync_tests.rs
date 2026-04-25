use std::sync::{Arc, Mutex};

use bson::doc;
use rango_core::{
    AnomalySignalHook, AuditSink, BoundedContextFilterHook, ControlPlane, NoopPromotionGateHook,
    PromotionRequest, RangoEngine, ReadRequest, RetrievalGateHook, TrustScoringHook, WriteContext,
    WritePayload, WriteValidationHook,
};
use rango_oplog::NullOplog;
use rango_storage::MemoryStorage;
use rango_types::{
    CollectionName, DocumentId, GovernanceDecision, MemoryTier, Mutation, MutationOp,
    PolicyDecision, RangoError, Revision,
};
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
    let id = engine_a
        .insert_one(&coll, doc! { "name": "Alice" })
        .unwrap();

    // Simulate network partition: both nodes get the doc
    let doc_a = engine_a.find_one(&coll, &id).unwrap().unwrap().data;

    // Node B applies the same doc (as if it received it via sync)
    let engine_b = setup("node-b");
    engine_b
        .apply_remote_mutation(&coll, doc_a.clone())
        .unwrap();

    // Both mutate concurrently
    engine_a
        .update_one(&coll, &id, doc! { "$set": { "name": "Alice Updated A" } })
        .unwrap();
    engine_b
        .update_one(&coll, &id, doc! { "$set": { "name": "Bob Updated B" } })
        .unwrap();

    // Get final docs
    let final_a = engine_a.find_one(&coll, &id).unwrap().unwrap().data;
    let final_b = engine_b.find_one(&coll, &id).unwrap().unwrap().data;

    // Now reconcile: B applies A's mutation, A applies B's mutation
    engine_b
        .apply_remote_mutation(&coll, final_a.clone())
        .unwrap();
    engine_a
        .apply_remote_mutation(&coll, final_b.clone())
        .unwrap();

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
fn test_apply_mutations_rejects_invalid_metadata() {
    let engine = setup("node-a");
    let coll = CollectionName::new("test");
    let doc_id = DocumentId::new_uuid_v7();

    let mutation = Mutation {
        op: MutationOp::Insert,
        collection: coll.0.clone(),
        doc_id: doc_id.clone(),
        patch: Some(doc! { "_id": doc_id.0.clone(), "name": "bad-meta" }),
        seq: 1,
        timestamp: bson::DateTime::now(),
        rev: Revision::now("node-a"),
        write_id: "bad-meta-write".to_string(),
        metadata: rango_types::MutationMetadata {
            id: doc_id,
            namespace: coll.0.clone(),
            tenant_id: String::new(),
            r#type: "state".to_string(),
            rev: Revision::now("node-a"),
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-a".to_string(),
            actor: "node-a".to_string(),
            lineage: "lineage".to_string(),
            schema_version: 1,
            trust_score: 0.8,
            verified: Some(true),
            expires_at: None,
        },
    };

    let result = engine.apply_mutations_deterministic(&coll, vec![mutation]);
    assert!(matches!(result, Err(RangoError::Sync(_))));
}

#[test]
fn test_tombstone_sync() {
    let engine_a = setup("node-a");
    let coll = CollectionName::new("test");

    let id = engine_a
        .insert_one(&coll, doc! { "name": "Alice" })
        .unwrap();
    engine_a.delete_one(&coll, &id).unwrap();

    // Verify deleted doc is not findable by default
    assert!(engine_a.find_one(&coll, &id).unwrap().is_none());

    // But exists as tombstone in raw storage
    let raw = engine_a.find_all_raw(&coll).unwrap();
    assert_eq!(raw.len(), 1);
    assert!(raw[0].get_bool("_deleted").unwrap());
}

#[test]
fn test_phase8_metrics_local_write_latency() {
    let engine = setup("node-a");
    let coll = CollectionName::new("metrics");

    let id = engine.insert_one(&coll, doc! { "name": "Alice" }).unwrap();
    engine
        .update_one(&coll, &id, doc! { "$set": { "name": "Alice 2" } })
        .unwrap();
    engine.delete_one(&coll, &id).unwrap();

    let snapshot = engine.metrics().snapshot();
    assert!(snapshot.local_write_latency_us_count >= 3);
    assert!(snapshot.local_write_latency_us_total > 0);
}

#[test]
fn test_phase8_metrics_replay_duration_and_drift() {
    let engine = setup("node-a");
    let coll = CollectionName::new("metrics-replay");

    let id_a = DocumentId::new_uuid_v7();
    let id_b = DocumentId::new_uuid_v7();
    let rev = Revision::now("node-a");

    let mut_a = Mutation {
        op: MutationOp::Insert,
        collection: coll.0.clone(),
        doc_id: id_a.clone(),
        patch: Some(doc! {
            "_id": id_a.0.clone(),
            "_rev": rev.to_string(),
            "name": "A"
        }),
        seq: 2,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: "write-1".to_string(),
        metadata: rango_types::MutationMetadata {
            id: id_a.clone(),
            namespace: coll.0.clone(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev: rev.clone(),
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-a".to_string(),
            actor: "node-a".to_string(),
            lineage: id_a.to_string(),
            schema_version: 1,
            trust_score: 0.9,
            verified: Some(true),
            expires_at: None,
        },
    };
    let mut_b = Mutation {
        op: MutationOp::Insert,
        collection: coll.0.clone(),
        doc_id: id_b.clone(),
        patch: Some(doc! {
            "_id": id_b.0.clone(),
            "_rev": rev.to_string(),
            "name": "B"
        }),
        seq: 1,
        timestamp: bson::DateTime::now(),
        rev,
        write_id: "write-1".to_string(),
        metadata: rango_types::MutationMetadata {
            id: id_b.clone(),
            namespace: coll.0.clone(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev: Revision::now("node-a"),
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-a".to_string(),
            actor: "node-a".to_string(),
            lineage: id_b.to_string(),
            schema_version: 1,
            trust_score: 0.9,
            verified: Some(true),
            expires_at: None,
        },
    };

    // Out-of-order + duplicate write_id should increment drift detection count.
    engine
        .apply_mutations_deterministic(&coll, vec![mut_a, mut_b])
        .unwrap();

    let snapshot = engine.metrics().snapshot();
    assert!(snapshot.replay_duration_us_count >= 1);
    assert!(snapshot.replay_duration_us_total > 0);
    assert!(snapshot.replay_drift_detection_count >= 1);
}

struct OrderedWriteHook {
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl WriteValidationHook for OrderedWriteHook {
    fn validate(&self, _ctx: &WriteContext, _payload: &WritePayload) -> GovernanceDecision {
        self.order.lock().unwrap().push("write.validate");
        GovernanceDecision {
            decision: PolicyDecision::Allow,
            reason: "ok".to_string(),
        }
    }
}

struct OrderedTrustHook {
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl TrustScoringHook for OrderedTrustHook {
    fn score(&self, _ctx: &WriteContext, _payload: &WritePayload) -> f64 {
        self.order.lock().unwrap().push("write.trust");
        0.95
    }
}

struct OrderedRetrievalHook {
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl RetrievalGateHook for OrderedRetrievalHook {
    fn allow(&self, _request: &ReadRequest) -> GovernanceDecision {
        self.order.lock().unwrap().push("read.gate");
        GovernanceDecision {
            decision: PolicyDecision::Allow,
            reason: "ok".to_string(),
        }
    }
}

struct OrderedFilterHook {
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl BoundedContextFilterHook for OrderedFilterHook {
    fn apply(
        &self,
        _request: &ReadRequest,
        candidates: Vec<bson::Document>,
    ) -> Vec<bson::Document> {
        self.order.lock().unwrap().push("read.filter");
        candidates
    }
}

struct OrderedAnomalyHook {
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl AnomalySignalHook for OrderedAnomalyHook {
    fn evaluate(&self, stage: &'static str, _decision: &GovernanceDecision) {
        self.order.lock().unwrap().push(match stage {
            "write.validate" => "write.anomaly.validate",
            "write.trust" => "write.anomaly.trust",
            "read.gate" => "read.anomaly",
            _ => "anomaly.other",
        });
    }
}

struct OrderedAuditSink {
    order: Arc<Mutex<Vec<&'static str>>>,
}

impl AuditSink for OrderedAuditSink {
    fn record(&self, stage: &'static str, _decision: &GovernanceDecision) {
        self.order.lock().unwrap().push(match stage {
            "write.validate" => "write.audit.validate",
            "write.trust" => "write.audit.trust",
            "read.gate" => "read.audit",
            _ => "audit.other",
        });
    }
}

#[test]
fn test_control_plane_hook_invocation_order_write_and_read() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let control_plane = ControlPlane::with_hooks(
        Arc::new(OrderedWriteHook {
            order: order.clone(),
        }),
        Arc::new(OrderedTrustHook {
            order: order.clone(),
        }),
        Arc::new(NoopPromotionGateHook),
        Arc::new(OrderedRetrievalHook {
            order: order.clone(),
        }),
        Arc::new(OrderedFilterHook {
            order: order.clone(),
        }),
        Arc::new(OrderedAnomalyHook {
            order: order.clone(),
        }),
        Arc::new(OrderedAuditSink {
            order: order.clone(),
        }),
    );

    let write_ctx = WriteContext {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        actor: "actor".to_string(),
        source: "source".to_string(),
        tier: MemoryTier::State,
    };
    let write_payload = WritePayload::State(doc! { "k": "v" });

    let write_decision = control_plane
        .write_path(&write_ctx, &write_payload)
        .unwrap();
    assert!(matches!(write_decision.decision, PolicyDecision::Allow));

    let read_request = ReadRequest {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        tier: MemoryTier::State,
        limit: 5,
    };
    let (_, filtered) = control_plane
        .read_path(&read_request, vec![doc! { "k": "v" }])
        .unwrap();
    assert_eq!(filtered.len(), 1);

    let order = order.lock().unwrap().clone();
    // Canonical runtime order: read.gate -> read.audit -> read.anomaly -> read.filter.
    assert_eq!(
        order,
        vec![
            "write.validate",
            "write.audit.validate",
            "write.anomaly.validate",
            "write.trust",
            "write.audit.trust",
            "write.anomaly.trust",
            "read.gate",
            "read.audit",
            "read.anomaly",
            "read.filter"
        ]
    );
}

#[test]
fn test_control_plane_promotion_path_is_explicit() {
    let control_plane = ControlPlane::default();
    let request = PromotionRequest {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        from: MemoryTier::Episodic,
        to: MemoryTier::Semantic,
        candidate_id: "candidate-1".to_string(),
    };
    let payload = WritePayload::State(doc! { "k": "v" });

    let (decision, sanitized) = control_plane.promotion_path(&request, &payload).unwrap();
    assert!(matches!(decision.decision, PolicyDecision::Allow));
    match sanitized {
        WritePayload::State(d) => assert_eq!(d.get_str("k").unwrap(), "v"),
        _ => panic!("expected state payload"),
    }
}

#[test]
fn test_control_plane_rejects_low_trust_writes() {
    let control_plane = ControlPlane::default();
    let write_ctx = WriteContext {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        actor: "actor".to_string(),
        source: "source".to_string(),
        tier: MemoryTier::State,
    };

    let payload = WritePayload::StateWithTrust {
        document: doc! { "content": "candidate" },
        trust_score: 0.1,
    };
    let decision = control_plane.write_path(&write_ctx, &payload).unwrap();
    assert!(matches!(decision.decision, PolicyDecision::Reject));
    assert!(decision.reason.starts_with("trust_score_below_threshold"));
}

#[test]
fn test_control_plane_rejects_invalid_trust_scores() {
    let control_plane = ControlPlane::default();
    let write_ctx = WriteContext {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        actor: "actor".to_string(),
        source: "source".to_string(),
        tier: MemoryTier::State,
    };

    let payload = WritePayload::StateWithTrust {
        document: doc! { "content": "candidate" },
        trust_score: f64::NAN,
    };
    let decision = control_plane.write_path(&write_ctx, &payload).unwrap();
    assert!(matches!(decision.decision, PolicyDecision::Reject));
    assert!(decision.reason.starts_with("invalid_trust_score:"));
}

#[test]
fn test_control_plane_read_path_rejects_invalid_limit() {
    let control_plane = ControlPlane::default();
    let read_request = ReadRequest {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        tier: MemoryTier::State,
        limit: 0,
    };

    let (decision, filtered) = control_plane
        .read_path(&read_request, vec![doc! { "k": "v" }])
        .unwrap();
    assert!(matches!(decision.decision, PolicyDecision::Reject));
    assert_eq!(decision.reason, "invalid_read_request:limit_must_be_positive");
    assert!(filtered.is_empty());
}

#[test]
fn test_control_plane_read_path_truncates_to_limit() {
    let control_plane = ControlPlane::default();
    let read_request = ReadRequest {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        tier: MemoryTier::State,
        limit: 1,
    };

    let (_decision, filtered) = control_plane
        .read_path(
            &read_request,
            vec![doc! { "id": "a" }, doc! { "id": "b" }, doc! { "id": "c" }],
        )
        .unwrap();
    assert_eq!(filtered.len(), 1);
}

#[test]
fn test_control_plane_promotion_path_rejects_non_semantic_route() {
    let control_plane = ControlPlane::default();
    let request = PromotionRequest {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        from: MemoryTier::State,
        to: MemoryTier::Semantic,
        candidate_id: "candidate-1".to_string(),
    };
    let payload = WritePayload::State(doc! { "k": "v" });

    let (decision, _sanitized) = control_plane.promotion_path(&request, &payload).unwrap();
    assert!(matches!(decision.decision, PolicyDecision::Reject));
    assert_eq!(
        decision.reason,
        "semantic_promotion_requires_episodic_to_semantic"
    );
}

#[test]
fn test_snapshot_restore_converges_with_full_replay() {
    let coll = CollectionName::new("snapshot");
    let source = setup("node-a");
    let id = source
        .insert_one(&coll, doc! { "name": "Alice", "v": 1 })
        .unwrap();
    source
        .update_one(&coll, &id, doc! { "$set": { "v": 2 } })
        .unwrap();
    let snapshot = source
        .create_snapshot(&coll, "tenant-a", "snap-1", 2)
        .unwrap();

    let rev = Revision::now("node-a");
    let replay_mutation = Mutation {
        op: MutationOp::Update,
        collection: coll.0.clone(),
        doc_id: id.clone(),
        patch: Some(doc! {
            "_id": id.0.clone(),
            "_rev": rev.to_string(),
            "name": "Alice",
            "v": 3
        }),
        seq: 3,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: "snapshot-replay-write".to_string(),
        metadata: rango_types::MutationMetadata {
            id: id.clone(),
            namespace: coll.0.clone(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-a".to_string(),
            actor: "node-a".to_string(),
            lineage: id.to_string(),
            schema_version: 1,
            trust_score: 0.9,
            verified: Some(true),
            expires_at: None,
        },
    };
    let audit_doc_id = DocumentId::new_uuid_v7();
    let audit_rev = Revision::now("node-a");
    let governance_audit_mutation = Mutation {
        op: MutationOp::Insert,
        collection: coll.0.clone(),
        doc_id: audit_doc_id.clone(),
        patch: Some(doc! {
            "_id": audit_doc_id.0.clone(),
            "_rev": audit_rev.to_string(),
            "stage": "write",
            "decision": "allow",
            "tenant_id": "tenant-a",
            "namespace": coll.0.clone(),
            "write_id": "snapshot-replay-write"
        }),
        seq: 4,
        timestamp: bson::DateTime::now(),
        rev: audit_rev.clone(),
        write_id: "snapshot-governance-audit".to_string(),
        metadata: rango_types::MutationMetadata {
            id: audit_doc_id.clone(),
            namespace: coll.0.clone(),
            tenant_id: "tenant-a".to_string(),
            r#type: "governance_audit".to_string(),
            rev: audit_rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-a".to_string(),
            actor: "node-a".to_string(),
            lineage: audit_doc_id.to_string(),
            schema_version: 1,
            trust_score: 1.0,
            verified: Some(true),
            expires_at: None,
        },
    };

    let restored = setup("node-a");
    restored
        .restore_from_snapshot(
            &coll,
            &snapshot,
            vec![replay_mutation.clone(), governance_audit_mutation.clone()],
        )
        .unwrap();

    let full_replay = setup("node-a");
    for doc in &snapshot.state {
        let doc_id = DocumentId::from_bson(doc.get("_id").unwrap().clone());
        full_replay
            .apply_remote_mutation(&coll, doc.clone())
            .unwrap();
        // Ensure equivalent materialization for pre-snapshot state.
        assert!(full_replay.find_one(&coll, &doc_id).unwrap().is_some());
    }
    full_replay
        .apply_mutations_deterministic(&coll, vec![replay_mutation, governance_audit_mutation])
        .unwrap();

    let restored_doc = restored.find_one(&coll, &id).unwrap().unwrap().data;
    let replay_doc = full_replay.find_one(&coll, &id).unwrap().unwrap().data;
    assert_eq!(restored_doc, replay_doc);
    let restored_raw = restored.find_all_raw(&coll).unwrap();
    let replay_raw = full_replay.find_all_raw(&coll).unwrap();
    assert_eq!(restored_raw.len(), replay_raw.len());
    assert!(restored_raw.iter().all(|doc| {
        doc.get_str("tenant_id")
            .map(|tenant| tenant == "tenant-a")
            .unwrap_or(true)
    }));
}

#[test]
fn test_idempotent_replay_across_multiple_batches() {
    let coll = CollectionName::new("idempotent");
    let engine = setup("node-a");
    let id = DocumentId::new_uuid_v7();
    let rev = Revision::now("node-a");
    let mutation = Mutation {
        op: MutationOp::Insert,
        collection: coll.0.clone(),
        doc_id: id.clone(),
        patch: Some(doc! {
            "_id": id.0.clone(),
            "_rev": rev.to_string(),
            "name": "dedup"
        }),
        seq: 1,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: "global-dedup-write".to_string(),
        metadata: rango_types::MutationMetadata {
            id: id.clone(),
            namespace: coll.0.clone(),
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
    };

    let first = engine
        .apply_mutations_deterministic(&coll, vec![mutation.clone()])
        .unwrap();
    let second = engine
        .apply_mutations_deterministic(&coll, vec![mutation])
        .unwrap();

    assert_eq!(first, 1);
    assert_eq!(second, 0);
}
