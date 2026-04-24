use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::Extension;
use axum::extract::Json;
use axum::http::{HeaderMap, HeaderValue};
use bson::doc;
use rango_oplog::Oplog;
use rango_server::routes::{ServerState, handle_pull, handle_push};
use rango_sync::protocol::{PullRequest, PushRequest};
use rango_types::{
    Checkpoint, DocumentId, Mutation, MutationMetadata, MutationOp, OplogEntry, RangoError,
    Revision,
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

fn make_headers(read_tier: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("X-Rango-Protocol-Version", HeaderValue::from_static("1"));
    headers.insert(
        "Authorization",
        HeaderValue::from_static("Bearer test-token"),
    );
    if let Some(tier) = read_tier {
        headers.insert(
            "X-Rango-Read-Tier",
            HeaderValue::from_str(tier).expect("valid tier header"),
        );
    }
    headers
}

fn make_mutation(memory_type: &str, write_id: &str) -> Mutation {
    let doc_id = DocumentId::new_uuid_v7();
    let rev = Revision::now("node-1");
    Mutation {
        op: MutationOp::Insert,
        collection: "memory".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(doc! { "kind": memory_type, "value": write_id }),
        seq: 0,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: write_id.to_string(),
        metadata: MutationMetadata {
            id: doc_id.clone(),
            namespace: "ns-a".to_string(),
            tenant_id: "tenant-a".to_string(),
            r#type: memory_type.to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "node-1".to_string(),
            actor: "node-1".to_string(),
            lineage: doc_id.to_string(),
            schema_version: 1,
            trust_score: 0.9,
            verified: Some(true),
            expires_at: None,
        },
    }
}

#[tokio::test]
async fn semantic_reads_are_opt_in_and_marked_derived_non_canonical() {
    let state = Arc::new(ServerState::new(Arc::new(InMemoryOplog::default())));
    state.add_token_with_tenant("test-token", "node-1", "tenant-a");

    let push_request = PushRequest {
        node_id: "node-1".to_string(),
        tenant_id: "tenant-a".to_string(),
        namespace: "ns-a".to_string(),
        mutations: vec![
            make_mutation("state", "w-state"),
            make_mutation("semantic_projection", "w-semantic"),
        ],
        last_checkpoint: Checkpoint::initial(),
    };

    let push = handle_push(
        Extension(state.clone()),
        make_headers(None),
        Json(push_request),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(push.accepted_seqs.len(), 2);

    let state_pull = handle_pull(
        Extension(state.clone()),
        make_headers(None),
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

    assert_eq!(state_pull.mutations.len(), 1);
    assert_eq!(state_pull.mutations[0].metadata.r#type, "state");

    let semantic_pull = handle_pull(
        Extension(state),
        make_headers(Some("semantic")),
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

    assert_eq!(semantic_pull.mutations.len(), 1);
    assert_eq!(
        semantic_pull.mutations[0].metadata.r#type,
        "semantic_projection"
    );
    let patch = semantic_pull.mutations[0].patch.as_ref().unwrap();
    assert_eq!(patch.get_bool("derived").unwrap(), true);
    assert_eq!(patch.get_bool("canonical").unwrap(), false);
}
