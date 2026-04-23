use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue};
use axum::Extension;
use bson::doc;
use rango_oplog::Oplog;
use rango_server::routes::{handle_push, ServerState};
use rango_sync::protocol::PushRequest;
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

fn make_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Rango-Protocol-Version", HeaderValue::from_static("1"));
    headers.insert("Authorization", HeaderValue::from_static("Bearer token-a"));
    headers
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

fn good_mutation(write_id: &str) -> Mutation {
    let mut mutation = low_trust_mutation(write_id);
    mutation.metadata.trust_score = 0.95;
    mutation
}

#[tokio::test]
async fn anomaly_bursts_transition_to_containment_and_then_reset_after_cooldown() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = Arc::new(ServerState {
        oplog,
        tokens: Mutex::new(HashMap::new()),
        non_owner_rejections: AtomicU64::new(0),
        cross_tenant_rejections: AtomicU64::new(0),
        control_plane: Arc::new(rango_core::ControlPlane::default()),
    });
    state.add_token_with_tenant("token-a", "node-1", "tenant-a");

    let mut reasons = Vec::new();
    for idx in 0..5 {
        let response = handle_push(
            Extension(state.clone()),
            make_headers(),
            Json(PushRequest {
                node_id: "node-1".to_string(),
                tenant_id: "tenant-a".to_string(),
                namespace: "ns-a".to_string(),
                mutations: vec![low_trust_mutation(&format!("reject-{idx}"))],
                last_checkpoint: Checkpoint::initial(),
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response.accepted_seqs.len(), 0);
        reasons.extend(response.audit.into_iter().map(|d| d.reason));
    }

    // RED expectation for Wave 0: runtime should enter deterministic containment mode.
    assert!(
        reasons
            .iter()
            .any(|r| r.contains("containment_throttle") || r.contains("containment_reject")),
        "expected containment transitions after anomaly burst",
    );

    tokio::time::sleep(Duration::from_millis(750)).await;

    let recovered = handle_push(
        Extension(state.clone()),
        make_headers(),
        Json(PushRequest {
            node_id: "node-1".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns-a".to_string(),
            mutations: vec![good_mutation("post-cooldown")],
            last_checkpoint: Checkpoint::initial(),
        }),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(
        recovered.accepted_seqs.len(),
        1,
        "good request should recover after containment cooldown",
    );
    assert!(
        recovered
            .audit
            .iter()
            .any(|d| matches!(d.decision, PolicyDecision::Allow)),
        "post-cooldown request should be allowed",
    );
}

