use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue};
use axum::Extension;
use bson::doc;
use rango_oplog::Oplog;
use rango_server::routes::{handle_promote, handle_pull, handle_push, ServerState};
use rango_sync::protocol::{PromoteRequest, PullRequest, PushRequest};
use rango_types::{
    Checkpoint, DocumentId, MemoryTier, Mutation, MutationMetadata, MutationOp, OplogEntry,
    OplogOrigin, RangoError, Revision,
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

fn make_headers(token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Rango-Protocol-Version", HeaderValue::from_static("1"));
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    headers
}

fn make_mutation(write_id: &str, tenant_id: &str, namespace: &str, trust_score: f64) -> Mutation {
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
            namespace: namespace.to_string(),
            tenant_id: tenant_id.to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-1".to_string(),
            actor: "node-1".to_string(),
            lineage: doc_id.to_string(),
            schema_version: 1,
            trust_score,
            verified: Some(true),
            expires_at: None,
        },
    }
}

#[tokio::test]
async fn governance_decisions_are_persisted_as_durable_audit_evidence() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = Arc::new(ServerState {
        oplog: oplog.clone(),
        tokens: Mutex::new(HashMap::new()),
        non_owner_rejections: AtomicU64::new(0),
        cross_tenant_rejections: AtomicU64::new(0),
        control_plane: Arc::new(rango_core::ControlPlane::default()),
    });
    state.add_token_with_tenant("token-a", "node-1", "tenant-a");

    let push_response = handle_push(
        Extension(state.clone()),
        make_headers("token-a"),
        Json(PushRequest {
            node_id: "node-1".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns-a".to_string(),
            mutations: vec![
                make_mutation("allow-1", "tenant-a", "ns-a", 0.9),
                make_mutation("reject-low-trust", "tenant-a", "ns-a", 0.1),
            ],
            last_checkpoint: Checkpoint::initial(),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(push_response.audit.len(), 2);

    let _ = handle_pull(
        Extension(state.clone()),
        make_headers("token-a"),
        Json(PullRequest {
            node_id: "node-1".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns-a".to_string(),
            since_checkpoint: Checkpoint::initial(),
        }),
    )
    .await
    .unwrap()
    .0;

    let _ = handle_promote(
        Extension(state.clone()),
        make_headers("token-a"),
        Json(PromoteRequest {
            node_id: "node-1".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns-a".to_string(),
            mutation: make_mutation("promote-allow", "tenant-a", "ns-a", 0.9),
            from_tier: MemoryTier::Episodic,
            to_tier: MemoryTier::Semantic,
            candidate_id: "candidate-1".to_string(),
            last_checkpoint: Checkpoint(push_response.new_checkpoint.0),
        }),
    )
    .await
    .unwrap()
    .0;

    let entries = oplog.read_since(1, 1000).unwrap();
    let audit_entries: Vec<&OplogEntry> = entries
        .iter()
        .filter(|entry| entry.mutation.metadata.r#type == "governance_audit")
        .collect();

    // RED expectation for Wave 0: decision evidence must be durable canonical substrate data.
    assert!(
        audit_entries
            .iter()
            .any(|entry| entry.mutation.patch.as_ref().is_some_and(|doc| {
                doc.get_str("decision").ok() == Some("allow")
                    && doc.get_str("stage").ok() == Some("write")
            })),
        "missing durable allow governance audit entry",
    );
    assert!(
        audit_entries
            .iter()
            .any(|entry| entry.mutation.patch.as_ref().is_some_and(|doc| {
                doc.get_str("decision").ok() == Some("reject")
                    && doc.get_str("stage").ok() == Some("write")
            })),
        "missing durable reject governance audit entry",
    );
    assert!(
        audit_entries
            .iter()
            .any(|entry| entry.mutation.patch.as_ref().is_some_and(|doc| {
                doc.get_str("stage").ok() == Some("read")
            })),
        "missing durable read governance audit entry",
    );
    assert!(
        audit_entries
            .iter()
            .any(|entry| entry.mutation.patch.as_ref().is_some_and(|doc| {
                doc.get_str("stage").ok() == Some("promotion")
            })),
        "missing durable promotion governance audit entry",
    );

    assert!(
        entries
            .iter()
            .all(|entry| matches!(entry.origin, OplogOrigin::Remote | OplogOrigin::Replay | OplogOrigin::Local)),
        "unexpected oplog origin variant encountered",
    );

    assert!(
        !audit_entries.is_empty(),
        "expected durable governance audit entries in oplog",
    );
    assert!(
        audit_entries.len() >= 4,
        "expected at least one audit entry per runtime stage (write/read/promotion)",
    );
    assert!(
        audit_entries.iter().all(|entry| {
            entry
                .mutation
                .patch
                .as_ref()
                .is_some_and(|doc| doc.get_str("tenant_id").ok() == Some("tenant-a"))
        }),
        "audit entries must include tenant linkage",
    );
}
