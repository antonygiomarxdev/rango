use std::sync::Arc;

use rango_oplog::FileOplog;
use rango_server::{app, routes::ServerState};
use rango_sync::client::SyncClient;
use rango_types::{Checkpoint, Mutation, MutationOp, Revision};
use bson::doc;

fn dummy_mutation(seq: u64) -> Mutation {
    Mutation {
        op: MutationOp::Insert,
        collection: "test".to_string(),
        doc_id: rango_types::DocumentId::new_uuid_v7(),
        patch: Some(doc! { "seq": seq as i64 }),
        seq,
        timestamp: bson::DateTime::now(),
        rev: Revision::now("client-1"),
        write_id: format!("write-{}", seq),
    }
}

#[tokio::test]
async fn test_e2e_push_and_pull() {
    // Start server on random port
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token("test-token", "client-1");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Give server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{}", port), "test-token");

    // Push 2 mutations
    let mutations = vec![dummy_mutation(1), dummy_mutation(2)];
    let push_resp = client.push("client-1", mutations, Checkpoint::initial()).await.unwrap();
    assert_eq!(push_resp.accepted_seqs.len(), 2);
    assert_eq!(push_resp.new_checkpoint.0, 2);

    // Pull from checkpoint 0
    let pull_resp = client.pull("client-1", Checkpoint::initial()).await.unwrap();
    assert_eq!(pull_resp.mutations.len(), 2);
    assert_eq!(pull_resp.new_checkpoint.0, 2);

    // Idempotency: push same mutations again
    let mutations = vec![dummy_mutation(1), dummy_mutation(2)];
    let push_resp2 = client.push("client-1", mutations, Checkpoint(2)).await.unwrap();
    // Server should return same seqs (idempotent)
    assert_eq!(push_resp2.accepted_seqs.len(), 2);

    // Pull from checkpoint 2 — should get nothing new
    let pull_resp2 = client.pull("client-1", Checkpoint(2)).await.unwrap();
    assert_eq!(pull_resp2.mutations.len(), 0);
    assert_eq!(pull_resp2.new_checkpoint.0, 2);
}

#[tokio::test]
async fn test_auth_failure() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token("test-token", "client-1");

    let router = app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{}", port), "bad-token");

    let result = client.push("client-1", vec![], Checkpoint::initial()).await;
    assert!(result.is_err());
}

fn temp_oplog_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join(format!("rango-e2e-oplog-{}.rgo", n))
        .to_string_lossy()
        .to_string()
}
