use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bson::doc;
use rango_core::{
    AnomalySignalHook, ControlPlane, NoopAuditSink, NoopBoundedContextFilterHook,
    NoopPromotionGateHook, NoopRetrievalGateHook, NoopTrustScoringHook, NoopWriteValidationHook,
};
use rango_oplog::Oplog;
use rango_server::{app, routes::ServerState};
use rango_sync::client::SyncClient;
use rango_types::GovernanceDecision;
use rango_types::{
    Checkpoint, DocumentId, Mutation, MutationMetadata, MutationOp, OplogEntry, PolicyDecision,
    RangoError, Revision,
};

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

/// Counter-based anomaly hook that records signals per stage for verification.
#[derive(Default)]
struct CounterAnomalyHook {
    signals: Mutex<Vec<(String, GovernanceDecision)>>,
}

impl AnomalySignalHook for CounterAnomalyHook {
    fn evaluate(&self, stage: &'static str, decision: &GovernanceDecision) {
        self.signals
            .lock()
            .unwrap()
            .push((stage.to_string(), decision.clone()));
    }
}

impl CounterAnomalyHook {
    fn count_rejects_at_stage(&self, stage: &str) -> usize {
        self.signals
            .lock()
            .unwrap()
            .iter()
            .filter(|(s, d)| s == stage && matches!(d.decision, PolicyDecision::Reject))
            .count()
    }

    fn count_signals(&self) -> usize {
        self.signals.lock().unwrap().len()
    }
}

fn low_trust_mutation(write_id: &str) -> Mutation {
    let doc_id = DocumentId::new_uuid_v7();
    let rev = Revision::now("node-1");
    Mutation {
        op: MutationOp::Insert,
        collection: "state".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(doc! { "payload": write_id }),
        seq: 0,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: write_id.to_string(),
        metadata: MutationMetadata {
            id: doc_id.clone(),
            namespace: "ns-a".to_string(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-1".to_string(),
            actor: "node-1".to_string(),
            lineage: doc_id.to_string(),
            schema_version: 1,
            trust_score: 0.1,
            verified: Some(true),
            expires_at: None,
        },
    }
}

fn cross_tenant_mutation(write_id: &str) -> Mutation {
    let mut mutation = low_trust_mutation(write_id);
    mutation.metadata.tenant_id = "tenant-b".to_string();
    mutation.metadata.trust_score = 0.95;
    mutation
}

#[tokio::test]
async fn anomaly_signals_emitted_for_low_trust_rejects() {
    let anomaly = Arc::new(CounterAnomalyHook::default());
    let control_plane = Arc::new(ControlPlane::with_hooks(
        Arc::new(NoopWriteValidationHook),
        Arc::new(NoopTrustScoringHook),
        Arc::new(NoopPromotionGateHook),
        Arc::new(NoopRetrievalGateHook),
        Arc::new(NoopBoundedContextFilterHook),
        anomaly.clone(),
        Arc::new(NoopAuditSink),
    ));

    let oplog = Arc::new(InMemoryOplog::default());
    let state = Arc::new(ServerState::with_control_plane(oplog, control_plane));
    state.add_token_with_tenant("token-a", "node-1", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    // Push low-trust mutation (should be rejected)
    let resp = client
        .push_scoped(
            "node-1",
            "tenant-a",
            "ns-a",
            vec![low_trust_mutation("low-trust-1")],
            Checkpoint::initial(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.accepted_seqs.len(),
        0,
        "low-trust mutation should be rejected"
    );
    assert!(
        anomaly.count_signals() > 0,
        "anomaly signals should be emitted for rejected operations"
    );
    assert!(
        anomaly.count_rejects_at_stage("write.trust") >= 1,
        "write.trust stage should emit reject anomaly signal"
    );
}

#[tokio::test]
async fn anomaly_signals_emitted_for_cross_tenant_attempts() {
    let anomaly = Arc::new(CounterAnomalyHook::default());
    let control_plane = Arc::new(ControlPlane::with_hooks(
        Arc::new(NoopWriteValidationHook),
        Arc::new(NoopTrustScoringHook),
        Arc::new(NoopPromotionGateHook),
        Arc::new(NoopRetrievalGateHook),
        Arc::new(NoopBoundedContextFilterHook),
        anomaly.clone(),
        Arc::new(NoopAuditSink),
    ));

    let oplog = Arc::new(InMemoryOplog::default());
    let state = Arc::new(ServerState::with_control_plane(oplog, control_plane));
    state.add_token_with_tenant("token-a", "node-1", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    // Push cross-tenant mutation (should be rejected at server level before control plane)
    let resp = client
        .push_scoped(
            "node-1",
            "tenant-a",
            "ns-a",
            vec![cross_tenant_mutation("cross-tenant-1")],
            Checkpoint::initial(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.accepted_seqs.len(),
        0,
        "cross-tenant mutation should be rejected"
    );
    assert!(
        resp.audit.iter().any(|d| d.reason.contains("cross_tenant")),
        "cross-tenant should produce audit reason"
    );
}

#[tokio::test]
async fn containment_gate_blocks_pull_during_reject_mode() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "node-1", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    // Trigger reject mode: 5 low-trust mutations in a row
    for i in 0..5 {
        let resp = client
            .push_scoped(
                "node-1",
                "tenant-a",
                "ns-a",
                vec![low_trust_mutation(&format!("burst-{i}"))],
                Checkpoint::initial(),
            )
            .await
            .unwrap();
        assert_eq!(resp.accepted_seqs.len(), 0);
    }

    // Pull should be blocked by containment gate in reject mode
    let pull_resp = client
        .pull_scoped("node-1", "tenant-a", "ns-a", Checkpoint::initial())
        .await
        .unwrap();

    assert!(
        pull_resp
            .audit
            .iter()
            .any(|d| d.reason.contains("containment_reject")),
        "pull should be blocked by containment gate when in reject mode, got audit: {:?}",
        pull_resp.audit
    );
    assert_eq!(
        pull_resp.mutations.len(),
        0,
        "no mutations should be returned during containment"
    );
}

#[tokio::test]
async fn containment_gate_blocks_promote_during_reject_mode() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "node-1", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    // Trigger reject mode: 5 low-trust mutations in a row
    for i in 0..5 {
        let resp = client
            .push_scoped(
                "node-1",
                "tenant-a",
                "ns-a",
                vec![low_trust_mutation(&format!("burst-{i}"))],
                Checkpoint::initial(),
            )
            .await
            .unwrap();
        assert_eq!(resp.accepted_seqs.len(), 0);
    }

    // Promote should be blocked by containment gate in reject mode
    // Use a valid promotion path (Episodic -> Semantic) so containment gate is evaluated
    let mut valid_mutation = low_trust_mutation("promote-during-containment");
    valid_mutation.metadata.trust_score = 0.95;
    let promote_resp = client
        .promote_scoped(
            "node-1",
            "tenant-a",
            "ns-a",
            valid_mutation,
            rango_types::MemoryTier::Episodic,
            rango_types::MemoryTier::Semantic,
            "candidate-1".to_string(),
            Checkpoint::initial(),
        )
        .await
        .unwrap();

    assert!(
        promote_resp
            .audit
            .iter()
            .any(|d| d.reason.contains("containment_reject")),
        "promote should be blocked by containment gate when in reject mode, got audit: {:?}",
        promote_resp.audit
    );
    assert_eq!(
        promote_resp.accepted_seqs.len(),
        0,
        "no promotions should be accepted during containment"
    );
}
