use std::sync::Arc;

use bson::doc;
use rango_oplog::FileOplog;
use rango_server::{app, routes::ServerState};
use rango_sync::client::SyncClient;
use rango_types::{Checkpoint, Mutation, MutationOp, Revision};

fn mutation_for(tenant_id: &str, namespace: &str, write_id: &str, trust_score: f64) -> Mutation {
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
            trust_score,
            verified: Some(true),
            expires_at: None,
        },
    }
}

#[tokio::test]
async fn cross_tenant_injection_and_poisoning_attempts_do_not_leak_across_sync_paths() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");
    state.add_token_with_tenant("token-b", "client-b", "tenant-b");

    let router = app(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let tenant_a = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");
    let tenant_b = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-b");

    let accepted = tenant_a
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-a",
            vec![mutation_for("tenant-a", "ns-a", "a-good-1", 0.95)],
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.accepted_seqs.len(), 1);

    let poisoning = tenant_a
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-a",
            vec![mutation_for("tenant-a", "ns-a", "a-poison-low-trust", 0.1)],
            Checkpoint(accepted.new_checkpoint.0),
        )
        .await
        .unwrap();
    assert_eq!(poisoning.accepted_seqs.len(), 0);

    let cross_tenant_injection = tenant_a
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-a",
            vec![mutation_for("tenant-b", "ns-a", "forged-tenant-b", 0.95)],
            Checkpoint(accepted.new_checkpoint.0),
        )
        .await
        .unwrap();
    assert_eq!(cross_tenant_injection.accepted_seqs.len(), 0);
    assert_eq!(cross_tenant_injection.rejected_cross_tenant_count, 1);

    let pull_a = tenant_a
        .pull_scoped("client-a", "tenant-a", "ns-a", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(
        pull_a.mutations.len(),
        1,
        "tenant-a should only see valid own mutations",
    );
    assert!(
        pull_a
            .mutations
            .iter()
            .all(|m| m.metadata.tenant_id == "tenant-a" && m.metadata.namespace == "ns-a"),
    );

    let pull_b = tenant_b
        .pull_scoped("client-b", "tenant-b", "ns-a", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(
        pull_b.mutations.len(),
        0,
        "tenant-b should not observe tenant-a mutations",
    );

    // RED expectation for Wave 0: poisoning responses must include deterministic containment/audit labels.
    assert!(
        poisoning.audit.iter().any(|d| d.reason.contains("poisoning")),
        "expected poisoning-specific reject reason",
    );
    assert!(
        cross_tenant_injection
            .audit
            .iter()
            .any(|d| d.reason.contains("cross_tenant")),
        "expected cross-tenant leakage reason code",
    );
}

fn temp_oplog_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir()
        .join(format!("rango-poisoning-oplog-{}-{}.rgo", pid, n))
        .to_string_lossy()
        .to_string()
}

