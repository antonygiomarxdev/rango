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
async fn all_boundary_violations_produce_audit_records() {
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

    let tenant_a = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-a");
    let tenant_b = SyncClient::new(format!("http://127.0.0.1:{port}"), "token-b");

    // 1. Cross-tenant mutation injection
    let cross_tenant_push = tenant_a
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-audit",
            vec![mutation_for("tenant-b", "ns-audit", "forged-b", 0.95)],
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(cross_tenant_push.accepted_seqs.len(), 0);
    assert!(
        cross_tenant_push.audit.iter().any(|d| d.reason.contains("cross_tenant")),
        "cross-tenant mutation should produce audit record"
    );

    // 2. Low-trust poisoning attempt
    let poisoning = tenant_a
        .push_scoped(
            "client-a",
            "tenant-a",
            "ns-audit",
            vec![mutation_for("tenant-a", "ns-audit", "poison", 0.05)],
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(poisoning.accepted_seqs.len(), 0);
    assert!(
        poisoning.audit.iter().any(|d| d.reason.contains("trust") || d.reason.contains("poison")),
        "low-trust poisoning should produce audit record"
    );

    // 3. Cross-tenant pull attempt (use tenant_b token but request tenant-a data)
    let cross_tenant_pull = tenant_b
        .pull_scoped("client-b", "tenant-a", "ns-audit", Checkpoint::initial())
        .await
        .unwrap();
    assert_eq!(cross_tenant_pull.mutations.len(), 0);
    // Pull rejection audit is returned in the response
    assert!(
        cross_tenant_pull.audit.iter().any(|d| d.reason.contains("tenant_mismatch")),
        "cross-tenant pull should produce audit record with tenant_mismatch reason"
    );

    // 4. Node ownership mismatch on push
    let owner_mismatch = tenant_a
        .push_scoped(
            "not-client-a",
            "tenant-a",
            "ns-audit",
            vec![mutation_for("tenant-a", "ns-audit", "wrong-owner", 0.95)],
            Checkpoint::initial(),
        )
        .await
        .unwrap();
    assert_eq!(owner_mismatch.accepted_seqs.len(), 0);
    assert!(
        owner_mismatch.audit.iter().any(|d| d.reason.contains("owner") || d.reason.contains("node")),
        "node ownership mismatch should produce audit record"
    );

    // Verify global state counters
    assert!(
        state.cross_tenant_rejections() >= 1,
        "cross_tenant_rejections counter should be incremented"
    );

    // Verify audit records are durable in oplog
    let all_entries = state.oplog.read_since(0, 1000).unwrap();
    let audit_entries: Vec<_> = all_entries
        .into_iter()
        .filter(|e| e.mutation.collection == "__governance_audit")
        .collect();

    assert!(
        !audit_entries.is_empty(),
        "audit records should be persisted to __governance_audit collection in oplog"
    );

    let stages: std::collections::HashSet<String> = audit_entries
        .iter()
        .filter_map(|e| {
            e.mutation
                .patch
                .as_ref()
                .and_then(|p: &bson::Document| p.get_str("stage").ok().map(|s: &str| s.to_string()))
        })
        .collect();

    assert!(
        stages.contains("write"),
        "push violations should be recorded in audit with 'write' stage, found stages: {:?}",
        stages
    );
}

fn temp_oplog_path() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir()
        .join(format!("rango-audit-completeness-oplog-{}-{}.rgo", pid, n))
        .to_string_lossy()
        .to_string()
}
