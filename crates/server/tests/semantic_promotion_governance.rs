use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::Extension;
use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue};
use bson::doc;
use rango_oplog::Oplog;
use rango_server::routes::{ServerState, handle_promote};
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
    headers.insert(
        "Authorization",
        HeaderValue::from_static("Bearer test-token"),
    );
    headers
}

fn make_promote_request(from_tier: MemoryTier, to_tier: MemoryTier) -> PromoteRequest {
    let doc_id = DocumentId::new_uuid_v7();
    let rev = Revision::now("node-1");
    PromoteRequest {
        node_id: "node-1".to_string(),
        tenant_id: "tenant-a".to_string(),
        namespace: "ns-a".to_string(),
        mutation: Mutation {
            op: MutationOp::Update,
            collection: "semantic".to_string(),
            doc_id: doc_id.clone(),
            patch: Some(doc! { "semantic": "derived fact" }),
            seq: 0,
            timestamp: bson::DateTime::now(),
            rev: rev.clone(),
            write_id: "semantic-promote-1".to_string(),
            metadata: MutationMetadata {
                id: doc_id.clone(),
                namespace: "ns-a".to_string(),
                tenant_id: "tenant-a".to_string(),
                r#type: "semantic_projection".to_string(),
                rev,
                created_at: bson::DateTime::now(),
                updated_at: bson::DateTime::now(),
                source: "node-1".to_string(),
                actor: "node-1".to_string(),
                lineage: doc_id.to_string(),
                schema_version: 1,
                trust_score: 0.95,
                verified: Some(true),
                expires_at: None,
            },
        },
        from_tier,
        to_tier,
        candidate_id: "candidate-1".to_string(),
        last_checkpoint: Checkpoint::initial(),
    }
}

#[tokio::test]
async fn semantic_promotion_requires_episodic_to_semantic_path() {
    let state = ServerState::new(Arc::new(InMemoryOplog::default()));
    state.add_token_with_tenant("test-token", "node-1", "tenant-a");

    let response = handle_promote(
        Extension(Arc::new(state)),
        make_headers(),
        Json(make_promote_request(
            MemoryTier::State,
            MemoryTier::Semantic,
        )),
    )
    .await
    .unwrap()
    .0;

    assert!(response.accepted_seqs.is_empty());
    assert_eq!(response.rejected_count, 1);
    assert_eq!(response.audit.len(), 1);
    assert!(matches!(response.audit[0].decision, PolicyDecision::Reject));
    assert!(
        response.audit[0]
            .reason
            .contains("semantic_promotion_requires_episodic_to_semantic"),
    );
}
