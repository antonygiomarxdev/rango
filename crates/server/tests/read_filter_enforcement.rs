use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue};
use axum::Extension;
use bson::{doc, Document};
use rango_core::{
    BoundedContextFilterHook, ControlPlane, NoopAnomalySignalHook, NoopAuditSink,
    NoopPromotionGateHook, NoopRetrievalGateHook, NoopTrustScoringHook, NoopWriteValidationHook,
    ReadRequest,
};
use rango_oplog::Oplog;
use rango_server::routes::{handle_pull, ServerState};
use rango_sync::protocol::PullRequest;
use rango_types::{
    Checkpoint, DocumentId, Mutation, MutationMetadata, MutationOp, OplogEntry, OplogOrigin,
    PolicyDecision, RangoError, Revision,
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
        let items = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.seq >= seq)
            .take(limit)
            .cloned()
            .collect();
        Ok(items)
    }

    fn mark_applied(&self, _seq: u64) -> Result<(), RangoError> {
        Ok(())
    }

    fn latest_seq(&self) -> Result<u64, RangoError> {
        Ok(self.seq.load(Ordering::Relaxed))
    }
}

struct AllowedOnlyFilter;

impl BoundedContextFilterHook for AllowedOnlyFilter {
    fn apply(&self, _request: &ReadRequest, candidates: Vec<Document>) -> Vec<Document> {
        candidates
            .into_iter()
            .filter(|candidate| candidate.get_bool("allowed").unwrap_or(false))
            .collect()
    }
}

fn make_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Rango-Protocol-Version", HeaderValue::from_static("1"));
    headers.insert("Authorization", HeaderValue::from_static("Bearer test-token"));
    headers
}

fn make_mutation(write_id: &str, allowed: bool) -> Mutation {
    let doc_id = DocumentId::new_uuid_v7();
    let rev = Revision::now("node-1");
    Mutation {
        op: MutationOp::Insert,
        collection: "state".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(doc! { "allowed": allowed, "value": write_id }),
        seq: 0,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: write_id.to_string(),
        metadata: MutationMetadata {
            id: doc_id.clone(),
            namespace: "ns".to_string(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-1".to_string(),
            actor: "node-1".to_string(),
            lineage: doc_id.to_string(),
            schema_version: 1,
            trust_score: 0.8,
            verified: Some(true),
            expires_at: None,
        },
    }
}

#[tokio::test]
async fn pull_payload_uses_control_plane_filtered_candidates() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = ServerState {
        oplog: oplog.clone(),
        tokens: Mutex::new(HashMap::new()),
        non_owner_rejections: AtomicU64::new(0),
        cross_tenant_rejections: AtomicU64::new(0),
        control_plane: Arc::new(ControlPlane::with_hooks(
            Arc::new(NoopWriteValidationHook),
            Arc::new(NoopTrustScoringHook),
            Arc::new(NoopPromotionGateHook),
            Arc::new(NoopRetrievalGateHook),
            Arc::new(AllowedOnlyFilter),
            Arc::new(NoopAnomalySignalHook),
            Arc::new(NoopAuditSink),
        )),
    };
    state.add_token_with_tenant("test-token", "node-1", "tenant-a");

    oplog
        .append(OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation: make_mutation("w1", true),
            origin: OplogOrigin::Remote,
            applied: false,
            snapshot_anchor: None,
        })
        .unwrap();
    oplog
        .append(OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation: make_mutation("w2", false),
            origin: OplogOrigin::Remote,
            applied: false,
            snapshot_anchor: None,
        })
        .unwrap();

    let response = handle_pull(
        Extension(Arc::new(state)),
        make_headers(),
        Json(PullRequest {
            node_id: "node-1".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns".to_string(),
            since_checkpoint: Checkpoint::initial(),
        }),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(response.mutations.len(), 1);
    assert_eq!(response.mutations[0].write_id, "w1");
    assert!(matches!(response.audit[0].decision, PolicyDecision::Allow));
    assert_eq!(response.new_checkpoint.0, 2);
    assert_eq!(response.mutations[0].collection, "state");
    assert!(matches!(response.mutations[0].op, MutationOp::Insert));
    assert_eq!(response.mutations[0].metadata.tenant_id, "tenant-a");
    assert_eq!(response.mutations[0].metadata.namespace, "ns");
}
