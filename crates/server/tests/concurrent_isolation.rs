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
async fn namespace_isolation_holds_under_concurrent_sync() {
    let oplog = Arc::new(FileOplog::new(temp_oplog_path()).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "client-a", "tenant-a");
    state.add_token_with_tenant("token-b", "client-b", "tenant-b");
    state.add_token_with_tenant("token-c", "client-c", "tenant-c");

    let router = app(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let tenant_a = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");
    let tenant_b = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-b");
    let tenant_c = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-c");

    // Spawn concurrent push operations across tenants and namespaces
    let a_ns1 = tokio::spawn({
        let client = tenant_a.clone();
        async move {
            client
                .push_scoped(
                    "client-a",
                    "tenant-a",
                    "ns-1",
                    vec![mutation_for("tenant-a", "ns-1", "a-ns1-1")],
                    Checkpoint::initial(),
                )
                .await
        }
    });

    let a_ns2 = tokio::spawn({
        let client = tenant_a.clone();
        async move {
            client
                .push_scoped(
                    "client-a",
                    "tenant-a",
                    "ns-2",
                    vec![mutation_for("tenant-a", "ns-2", "a-ns2-1")],
                    Checkpoint::initial(),
                )
                .await
        }
    });

    let b_ns1 = tokio::spawn({
        let client = tenant_b.clone();
        async move {
            client
                .push_scoped(
                    "client-b",
                    "tenant-b",
                    "ns-1",
                    vec![mutation_for("tenant-b", "ns-1", "b-ns1-1")],
                    Checkpoint::initial(),
                )
                .await
        }
    });

    let c_forge_a = tokio::spawn({
        let client = tenant_c.clone();
        async move {
            client
                .push_scoped(
                    "client-c",
                    "tenant-c",
                    "ns-1",
                    vec![mutation_for("tenant-a", "ns-1", "forged-a")],
                    Checkpoint::initial(),
                )
                .await
        }
    });

    let (a_ns1_res, a_ns2_res, b_ns1_res, c_forge_res) = tokio::join!(a_ns1, a_ns2, b_ns1, c_forge_a);

    let a_ns1_resp = a_ns1_res.unwrap().unwrap();
    let a_ns2_resp = a_ns2_res.unwrap().unwrap();
    let b_ns1_resp = b_ns1_res.unwrap().unwrap();
    let c_forge_resp = c_forge_res.unwrap().unwrap();

    assert_eq!(a_ns1_resp.accepted_seqs.len(), 1, "tenant-a ns-1 push should succeed");
    assert_eq!(a_ns2_resp.accepted_seqs.len(), 1, "tenant-a ns-2 push should succeed");
    assert_eq!(b_ns1_resp.accepted_seqs.len(), 1, "tenant-b ns-1 push should succeed");
    assert_eq!(c_forge_resp.accepted_seqs.len(), 0, "forged tenant-a mutation should be rejected");
    assert_eq!(c_forge_resp.rejected_cross_tenant_count, 1, "forged mutation should count as cross-tenant rejection");

    // Verify namespace isolation: each tenant only sees their own data
    let pull_a_ns1 = tenant_a
        .pull_scoped("client-a", "tenant-a", "ns-1", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(pull_a_ns1.mutations.len(), 1, "tenant-a ns-1 should see exactly 1 mutation");
    assert_eq!(
        pull_a_ns1.mutations[0].metadata.tenant_id, "tenant-a",
        "tenant-a pull should only contain tenant-a data"
    );

    let pull_a_ns2 = tenant_a
        .pull_scoped("client-a", "tenant-a", "ns-2", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(pull_a_ns2.mutations.len(), 1, "tenant-a ns-2 should see exactly 1 mutation");
    assert_eq!(
        pull_a_ns2.mutations[0].metadata.namespace, "ns-2",
        "tenant-a ns-2 pull should only contain ns-2 data"
    );

    let pull_b_ns1 = tenant_b
        .pull_scoped("client-b", "tenant-b", "ns-1", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(pull_b_ns1.mutations.len(), 1, "tenant-b ns-1 should see exactly 1 mutation");
    assert_eq!(
        pull_b_ns1.mutations[0].metadata.tenant_id, "tenant-b",
        "tenant-b pull should only contain tenant-b data"
    );

    let pull_c_ns1 = tenant_c
        .pull_scoped("client-c", "tenant-c", "ns-1", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(pull_c_ns1.mutations.len(), 0, "tenant-c ns-1 should see no mutations (forgery rejected)");

    // Verify audit records exist for the rejection
    assert!(
        c_forge_resp.audit.iter().any(|d| d.reason.contains("cross_tenant")),
        "forged cross-tenant mutation should produce audit record with cross_tenant reason"
    );

    // Verify checkpoint isolation: each namespace advances independently
    assert_ne!(
        pull_a_ns1.new_checkpoint.0, pull_a_ns2.new_checkpoint.0,
        "different namespaces should have independent checkpoint progression"
    );

    // Verify no cross-tenant leakage in state
    assert_eq!(
        state.cross_tenant_rejections(), 1,
        "exactly one cross-tenant rejection should be recorded in server state"
    );
}

#[tokio::test]
async fn concurrent_push_pull_same_namespace_preserves_consistency() {
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

    // Initial push
    let push1 = client
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-concurrent",
            vec![mutation_for("tenant-a", "ns-concurrent", "concurrent-1")],
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(push1.accepted_seqs.len(), 1);

    // Concurrent push and pull on same namespace
    let push2 = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .push_scoped(
                    "client-a",
                    "tenant-a",
                    "ns-concurrent",
                    vec![mutation_for("tenant-a", "ns-concurrent", "concurrent-2")],
                    Checkpoint(push1.new_checkpoint.0),
                )
                .await
        }
    });

    let pull1 = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .pull_scoped("client-a", "tenant-a", "ns-concurrent", Checkpoint::initial())
                .await
        }
    });

    let (push2_res, pull1_res) = tokio::join!(push2, pull1);
    let push2_resp = push2_res.unwrap().unwrap();
    let pull1_resp = pull1_res.unwrap().unwrap();

    assert_eq!(push2_resp.accepted_seqs.len(), 1, "concurrent push should succeed");
    assert!(
        pull1_resp.mutations.len() >= 1,
        "concurrent pull should see at least the first mutation"
    );
    assert!(
        pull1_resp.mutations.iter().all(|m| m.metadata.tenant_id == "tenant-a" && m.metadata.namespace == "ns-concurrent"),
        "all pulled mutations should belong to tenant-a ns-concurrent"
    );
}

fn temp_oplog_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir()
        .join(format!("rango-concurrent-isolation-oplog-{}-{}.rgo", pid, n))
        .to_string_lossy()
        .to_string()
}
