use std::sync::Arc;

use bson::doc;
use rango_oplog::FileOplog;
use rango_server::{app, routes::ServerState};
use rango_sync::client::SyncClient;
use rango_types::{Checkpoint, Mutation, MutationOp, Revision};

fn dummy_mutation(seq: u64) -> Mutation {
    let doc_id = rango_types::DocumentId::new_uuid_v7();
    let rev = Revision::now("client-1");
    Mutation {
        op: MutationOp::Insert,
        collection: "test".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(doc! { "seq": seq as i64 }),
        seq,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: format!("write-{}", seq),
        metadata: rango_types::MutationMetadata {
            id: doc_id.clone(),
            namespace: "test".to_string(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "client-1".to_string(),
            actor: "client-1".to_string(),
            lineage: doc_id.to_string(),
            schema_version: 1,
            trust_score: 0.8,
            verified: Some(true),
            expires_at: None,
        },
    }
}

#[tokio::test]
async fn test_e2e_push_and_pull() {
    // Start server on random port
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("test-token", "client-1", "tenant-a");

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
    let push_resp = client
        .push_scoped(
            "client-1",
            "tenant-a",
            "test",
            mutations,
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(push_resp.accepted_seqs.len(), 2);
    assert!(push_resp.new_checkpoint.0 >= 2);
    assert_eq!(push_resp.rejected_non_owner_count, 0);
    assert_eq!(push_resp.rejected_cross_tenant_count, 0);

    // Pull from checkpoint 0
    let pull_resp = client
        .pull_scoped("client-1", "tenant-a", "test", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(pull_resp.mutations.len(), 2);
    assert!(pull_resp.new_checkpoint.0 >= push_resp.new_checkpoint.0);

    // Idempotency: push same mutations again
    let mutations = vec![dummy_mutation(1), dummy_mutation(2)];
    let push_resp2 = client
        .push_scoped(
            "client-1",
            "tenant-a",
            "test",
            mutations,
            push_resp.new_checkpoint,
        )
        .await
        .unwrap();
    // Server should return same seqs (idempotent)
    assert_eq!(push_resp2.accepted_seqs.len(), 2);
    assert_eq!(push_resp2.rejected_non_owner_count, 0);
    assert_eq!(push_resp2.rejected_cross_tenant_count, 0);

    // Pull from checkpoint 2 — should get nothing new
    let pull_resp2 = client
        .pull_scoped("client-1", "tenant-a", "test", push_resp2.new_checkpoint)
        .await
        .unwrap();
    assert_eq!(pull_resp2.mutations.len(), 0);
    assert!(pull_resp2.new_checkpoint.0 >= push_resp2.new_checkpoint.0);

    // Non-owner write attempt: token owner is client-1, request claims client-2.
    let rejected = client
        .push_scoped(
            "client-2",
            "tenant-a",
            "test",
            vec![dummy_mutation(3)],
            Checkpoint(2),
        )
        .await
        .unwrap();
    assert_eq!(rejected.accepted_seqs.len(), 0);
    assert_eq!(rejected.rejected_non_owner_count, 1);
    assert_eq!(state.non_owner_rejections(), 1);

    let cross_tenant = client
        .push_scoped(
            "client-1",
            "tenant-b",
            "test",
            vec![dummy_mutation(4)],
            Checkpoint(2),
        )
        .await
        .unwrap();
    assert_eq!(cross_tenant.accepted_seqs.len(), 0);
    assert_eq!(cross_tenant.rejected_cross_tenant_count, 1);
    assert_eq!(state.cross_tenant_rejections(), 1);

    let mut low_trust = dummy_mutation(5);
    low_trust.metadata.trust_score = 0.1;
    let low_trust_resp = client
        .push_scoped(
            "client-1",
            "tenant-a",
            "test",
            vec![low_trust],
            Checkpoint(2),
        )
        .await
        .unwrap();
    assert_eq!(low_trust_resp.accepted_seqs.len(), 0);
    assert!(
        low_trust_resp
            .audit
            .iter()
            .any(|d| d.reason.contains("poisoning_low_trust"))
    );
}

#[tokio::test]
async fn test_auth_failure() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("test-token", "client-1", "tenant-a");

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

#[tokio::test]
async fn test_duplicate_write_id_remains_idempotent_with_out_of_order_batch() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("test-token", "client-1", "tenant-a");

    let router = app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{}", port), "test-token");
    let mut m1 = dummy_mutation(10);
    let mut m2 = dummy_mutation(9);
    m1.write_id = "same-write".to_string();
    m2.write_id = "same-write".to_string();

    let push = client
        .push_scoped(
            "client-1",
            "tenant-a",
            "test",
            vec![m1, m2], // intentionally out-of-order seq values
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(push.accepted_seqs.len(), 2);
    assert_eq!(push.accepted_seqs[0], push.accepted_seqs[1]);

    let pull = client
        .pull_scoped("client-1", "tenant-a", "test", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(pull.mutations.len(), 1);
}

#[tokio::test]
async fn test_cross_tenant_pull_isolation_prevents_leakage() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");
    state.add_token_with_tenant("token-b", "client-b", "tenant-b");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client_a = SyncClient::new(format!("http://127.0.0.1:{}", port), "token-a");
    let client_b = SyncClient::new(format!("http://127.0.0.1:{}", port), "token-b");

    let push = client_a
        .push_scoped(
            "client-a",
            "tenant-a",
            "test",
            vec![dummy_mutation(20), dummy_mutation(21)],
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(push.accepted_seqs.len(), 2);

    let pull_a = client_a
        .pull_scoped("client-a", "tenant-a", "test", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(pull_a.mutations.len(), 2);

    let pull_b = client_b
        .pull_scoped("client-b", "tenant-b", "test", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(pull_b.mutations.len(), 0);
}

fn temp_oplog_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir()
        .join(format!("rango-e2e-oplog-{}-{}.rgo", pid, n))
        .to_string_lossy()
        .to_string()
}
