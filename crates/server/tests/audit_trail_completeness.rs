use std::sync::Arc;

use bson::doc;
use rango_oplog::FileOplog;
use rango_server::{app, routes::ServerState};
use rango_sync::client::SyncClient;
use rango_types::{Checkpoint, MemoryTier, Mutation, MutationOp, Revision};

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

fn count_audit_records(state: &ServerState) -> usize {
    state
        .oplog
        .read_since(0, 1000)
        .unwrap()
        .into_iter()
        .filter(|e| e.mutation.collection == "__governance_audit")
        .count()
}

#[tokio::test]
async fn push_node_mismatch_emits_audit_record() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    let before = count_audit_records(&state);

    // Push with wrong node_id
    let resp = client
        .push_scoped(
            "wrong-node",
            "tenant-a",
            "ns-audit",
            vec![mutation_for("tenant-a", "ns-audit", "test-1")],
            Checkpoint::initial(),
        )
        .await
        .unwrap();

    assert_eq!(resp.accepted_seqs.len(), 0);
    assert_eq!(resp.rejected_non_owner_count, 1);

    let after = count_audit_records(&state);
    assert!(
        after > before,
        "node_mismatch push should emit persistent audit record, before={before} after={after}"
    );
}

#[tokio::test]
async fn promote_node_mismatch_emits_audit_record() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    let before = count_audit_records(&state);

    // Promote with wrong node_id
    let resp = client
        .promote_scoped(
            "wrong-node",
            "tenant-a",
            "ns-audit",
            mutation_for("tenant-a", "ns-audit", "promote-1"),
            MemoryTier::State,
            MemoryTier::Episodic,
            "candidate-1".to_string(),
            Checkpoint::initial(),
        )
        .await
        .unwrap();

    assert_eq!(resp.accepted_seqs.len(), 0);
    assert_eq!(resp.rejected_count, 1);

    let after = count_audit_records(&state);
    assert!(
        after > before,
        "promote node_mismatch should emit persistent audit record, before={before} after={after}"
    );
}

#[tokio::test]
async fn promote_tenant_mismatch_emits_audit_record() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    let before = count_audit_records(&state);

    // Promote with wrong tenant_id (using token-a which belongs to tenant-a)
    let resp = client
        .promote_scoped(
            "client-a",
            "tenant-b",
            "ns-audit",
            mutation_for("tenant-b", "ns-audit", "promote-2"),
            MemoryTier::State,
            MemoryTier::Episodic,
            "candidate-2".to_string(),
            Checkpoint::initial(),
        )
        .await
        .unwrap();

    assert_eq!(resp.accepted_seqs.len(), 0);
    assert_eq!(resp.rejected_count, 1);

    let after = count_audit_records(&state);
    assert!(
        after > before,
        "promote tenant_mismatch should emit persistent audit record, before={before} after={after}"
    );
}

#[tokio::test]
async fn promote_invalid_metadata_emits_audit_record() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    let before = count_audit_records(&state);

    // Promote with forged metadata tenant
    let mut bad_mutation = mutation_for("tenant-a", "ns-audit", "promote-3");
    bad_mutation.metadata.tenant_id = "tenant-b".to_string(); // mismatch with request scope

    let resp = client
        .promote_scoped(
            "client-a",
            "tenant-a",
            "ns-audit",
            bad_mutation,
            MemoryTier::State,
            MemoryTier::Episodic,
            "candidate-3".to_string(),
            Checkpoint::initial(),
        )
        .await
        .unwrap();

    assert_eq!(resp.accepted_seqs.len(), 0);
    assert_eq!(resp.rejected_count, 1);

    let after = count_audit_records(&state);
    assert!(
        after > before,
        "promote cross_tenant_mutation should emit persistent audit record, before={before} after={after}"
    );
}

#[tokio::test]
async fn audit_records_survive_restart() {
    let path = temp_oplog_path();
    let oplog = Arc::new(FileOplog::new(path.clone()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");

    // Emit some audit records via rejected operations
    let _ = client
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-restart",
            vec![mutation_for("tenant-b", "ns-restart", "forged")],
            Checkpoint::initial(),
        )
        .await
        .unwrap();

    let audit_before = count_audit_records(&state);
    assert!(audit_before > 0, "should have audit records before restart");

    // Simulate restart by creating a new ServerState from same oplog
    let new_oplog = Arc::new(FileOplog::new(path).unwrap());
    let new_state = Arc::new(ServerState::new(new_oplog));

    let audit_after = count_audit_records(&new_state);
    assert_eq!(
        audit_after, audit_before,
        "audit records should survive oplog restart/replay"
    );
}

fn temp_oplog_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir()
        .join(format!("rango-audit-trail-oplog-{}-{}.rgo", pid, n))
        .to_string_lossy()
        .to_string()
}
