use std::sync::Arc;

use bson::doc;
use rango_oplog::FileOplog;
use rango_server::{app, routes::ServerState};
use rango_sync::client::SyncClient;
use rango_types::{Checkpoint, Mutation, MutationOp, Revision};

fn mutation_for(tenant_id: &str, namespace: &str, write_id: &str) -> Mutation {
    let doc_id = rango_types::DocumentId::new_uuid_v7();
    let rev = Revision::now("client-a");
    Mutation {
        op: MutationOp::Insert,
        collection: "state".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(doc! { "payload": write_id }),
        seq: 0,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: write_id.to_string(),
        metadata: rango_types::MutationMetadata {
            id: doc_id.clone(),
            namespace: namespace.to_string(),
            tenant_id: tenant_id.to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "client-a".to_string(),
            actor: "client-a".to_string(),
            lineage: doc_id.to_string(),
            schema_version: 1,
            trust_score: 0.95,
            verified: Some(true),
            expires_at: None,
        },
    }
}

#[tokio::test]
async fn checkpoints_are_scoped_per_tenant_and_namespace() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");

    let router = app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    let ns_a_push = client
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-a",
            vec![mutation_for("tenant-a", "ns-a", "a-1")],
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(ns_a_push.accepted_seqs.len(), 1);

    let ns_b_push = client
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-b",
            vec![mutation_for("tenant-a", "ns-b", "b-1")],
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(ns_b_push.accepted_seqs.len(), 1);
    assert!(
        ns_b_push.new_checkpoint.0 > ns_a_push.new_checkpoint.0,
        "ns-b should advance its own scope",
    );

    let node_mismatch_ns_a = client
        .push_scoped(
            "not-owner",
            "tenant-a",
            "ns-a",
            Vec::new(),
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(node_mismatch_ns_a.rejected_non_owner_count, 1);
    assert_eq!(
        node_mismatch_ns_a.new_checkpoint.0, ns_a_push.new_checkpoint.0,
        "ns-a checkpoint must not jump due to ns-b activity on owner mismatch path",
    );

    let ns_a_pull = client
        .pull_scoped("client-a", "tenant-a", "ns-a", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(ns_a_pull.mutations.len(), 1);
}

fn temp_oplog_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir()
        .join(format!("rango-checkpoint-scope-oplog-{}-{}.rgo", pid, n))
        .to_string_lossy()
        .to_string()
}
