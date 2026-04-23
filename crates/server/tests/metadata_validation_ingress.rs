use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue};
use axum::Extension;
use bson::doc;
use rango_oplog::Oplog;
use rango_server::routes::{handle_push, ServerState};
use rango_sync::protocol::PushRequest;
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

fn make_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Rango-Protocol-Version", HeaderValue::from_static("1"));
    headers.insert("Authorization", HeaderValue::from_static("Bearer test-token"));
    headers
}

fn make_mutation(write_id: &str) -> Mutation {
    let doc_id = DocumentId::new_uuid_v7();
    let rev = Revision::now("node-1");
    Mutation {
        op: MutationOp::Insert,
        collection: "state".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(doc! { "value": write_id }),
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
async fn push_rejects_invalid_metadata_before_append() {
    let oplog = Arc::new(InMemoryOplog::default());
    let state = ServerState {
        oplog: oplog.clone(),
        tokens: Mutex::new(HashMap::new()),
        non_owner_rejections: AtomicU64::new(0),
        cross_tenant_rejections: AtomicU64::new(0),
        control_plane: Arc::new(rango_core::ControlPlane::default()),
    };
    state.add_token_with_tenant("test-token", "node-1", "tenant-a");

    let mut invalid = make_mutation("invalid");
    invalid.metadata.source = String::new();
    let valid = make_mutation("valid");

    let response = handle_push(
        Extension(Arc::new(state)),
        make_headers(),
        Json(PushRequest {
            node_id: "node-1".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns".to_string(),
            mutations: vec![invalid, valid],
            last_checkpoint: Checkpoint::initial(),
        }),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(response.accepted_seqs.len(), 1);
    assert_eq!(response.new_checkpoint.0, 1);
    assert_eq!(response.rejected_cross_tenant_count, 0);
    assert_eq!(response.rejected_non_owner_count, 0);
    assert_eq!(response.audit.len(), 2);
    assert!(response.audit.iter().any(|decision| {
        matches!(decision.decision, PolicyDecision::Reject)
            && decision.reason.contains("invalid_metadata:")
    }));
    assert!(response.audit.iter().any(|decision| {
        matches!(decision.decision, PolicyDecision::Allow)
            && decision.reason.contains("trust_score:")
    }));

    let persisted = oplog.read_since(1, 100).unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].mutation.write_id, "valid");
    assert!(matches!(persisted[0].origin, OplogOrigin::Remote));
}
