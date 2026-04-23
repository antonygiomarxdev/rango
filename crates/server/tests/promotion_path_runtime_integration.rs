use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue};
use axum::Extension;
use bson::doc;
use rango_core::{
    ControlPlane, NoopAnomalySignalHook, NoopAuditSink, NoopBoundedContextFilterHook,
    NoopRetrievalGateHook, NoopTrustScoringHook, NoopWriteValidationHook, PromotionGateHook,
    PromotionRequest as CorePromotionRequest, WritePayload,
};
use rango_oplog::Oplog;
use rango_server::routes::{handle_promote, ServerState};
use rango_sync::protocol::PromoteRequest;
use rango_types::{
    Checkpoint, DocumentId, MemoryTier, Mutation, MutationMetadata, MutationOp, OplogEntry,
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

struct RecordingPromotionHook {
    decisions: Mutex<Vec<PolicyDecision>>,
}

impl RecordingPromotionHook {
    fn new(decisions: Vec<PolicyDecision>) -> Self {
        Self {
            decisions: Mutex::new(decisions),
        }
    }
}

impl PromotionGateHook for RecordingPromotionHook {
    fn sanitize(&self, _request: &CorePromotionRequest, payload: &WritePayload) -> WritePayload {
        payload.clone()
    }

    fn allow(
        &self,
        _request: &CorePromotionRequest,
        _payload: &WritePayload,
    ) -> rango_types::GovernanceDecision {
        let decision = self.decisions.lock().unwrap().remove(0);
        match decision {
            PolicyDecision::Allow => rango_types::GovernanceDecision {
                decision,
                reason: "promote_allow".to_string(),
            },
            PolicyDecision::Sanitize => rango_types::GovernanceDecision {
                decision,
                reason: "promote_sanitize".to_string(),
            },
            PolicyDecision::Reject => rango_types::GovernanceDecision {
                decision,
                reason: "promote_reject".to_string(),
            },
        }
    }
}

fn make_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Rango-Protocol-Version", HeaderValue::from_static("1"));
    headers.insert("Authorization", HeaderValue::from_static("Bearer test-token"));
    headers
}

fn make_promote_request(write_id: &str, score: f64) -> PromoteRequest {
    let doc_id = DocumentId::new_uuid_v7();
    let rev = Revision::now("node-1");
    PromoteRequest {
        node_id: "node-1".to_string(),
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        mutation: Mutation {
            op: MutationOp::Update,
            collection: "state".to_string(),
            doc_id: doc_id.clone(),
            patch: Some(doc! { "memory": "promoted", "write_id": write_id }),
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
                trust_score: score,
                verified: Some(true),
                expires_at: None,
            },
        },
        from_tier: MemoryTier::Episodic,
        to_tier: MemoryTier::Semantic,
        candidate_id: format!("candidate-{write_id}"),
        last_checkpoint: Checkpoint::initial(),
    }
}

#[tokio::test]
async fn promote_runtime_enforces_promotion_path_allow_then_reject_with_audit() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = ServerState::with_control_plane(
        oplog.clone(),
        Arc::new(ControlPlane::with_hooks(
            Arc::new(NoopWriteValidationHook),
            Arc::new(NoopTrustScoringHook),
            Arc::new(RecordingPromotionHook::new(vec![
                PolicyDecision::Allow,
                PolicyDecision::Reject,
            ])),
            Arc::new(NoopRetrievalGateHook),
            Arc::new(NoopBoundedContextFilterHook),
            Arc::new(NoopAnomalySignalHook),
            Arc::new(NoopAuditSink),
        )),
    );
    state.add_token_with_tenant("test-token", "node-1", "tenant-a");
    let state = Arc::new(state);

    let allow_response = handle_promote(
        Extension(state.clone()),
        make_headers(),
        Json(make_promote_request("w-allow", 0.95)),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(allow_response.accepted_seqs.len(), 1);
    assert_eq!(allow_response.rejected_count, 0);
    assert_eq!(allow_response.audit.len(), 1);
    assert!(matches!(
        allow_response.audit[0].decision,
        PolicyDecision::Allow
    ));
    assert_eq!(allow_response.audit[0].reason, "promote_allow");

    let reject_response = handle_promote(
        Extension(state.clone()),
        make_headers(),
        Json(make_promote_request("w-reject", 0.95)),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(reject_response.accepted_seqs.len(), 0);
    assert_eq!(reject_response.rejected_count, 1);
    assert_eq!(reject_response.audit.len(), 1);
    assert!(matches!(
        reject_response.audit[0].decision,
        PolicyDecision::Reject
    ));
    assert_eq!(reject_response.audit[0].reason, "promote_reject");

    let persisted = oplog.read_since(1, 100).unwrap();
    let state_entries: Vec<_> = persisted
        .iter()
        .filter(|entry| entry.mutation.metadata.r#type == "state")
        .collect();
    assert_eq!(state_entries.len(), 1);
    assert_eq!(state_entries[0].mutation.write_id, "w-allow");
}

#[tokio::test]
async fn promote_runtime_rejects_invalid_metadata_before_promotion_path() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = ServerState::new(oplog.clone());
    state.add_token_with_tenant("test-token", "node-1", "tenant-a");

    let mut invalid = make_promote_request("w-invalid", 0.9);
    invalid.mutation.metadata.source = String::new();

    let response = handle_promote(
        Extension(Arc::new(state)),
        make_headers(),
        Json(invalid),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(response.accepted_seqs.len(), 0);
    assert_eq!(response.rejected_count, 1);
    assert_eq!(response.audit.len(), 1);
    assert!(matches!(response.audit[0].decision, PolicyDecision::Reject));
    assert!(response.audit[0].reason.contains("invalid_metadata:"));

    let persisted = oplog.read_since(1, 100).unwrap();
    let state_entries: Vec<_> = persisted
        .iter()
        .filter(|entry| entry.mutation.metadata.r#type == "state")
        .collect();
    assert!(state_entries.is_empty());
}
