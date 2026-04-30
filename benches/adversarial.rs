use bson::doc;
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use rand::{Rng, SeedableRng};
use rango_core::{ControlPlane, WriteContext, WritePayload};
use rango_oplog::{FileOplog, Oplog};
use rango_server::routes::{AuthPrincipal, ServerState, scoped_latest_checkpoint};
use rango_sync::protocol::{PullRequest, PushRequest};
use rango_types::{
    Checkpoint, DocumentId, GovernanceDecision, MemoryTier, Mutation, MutationMetadata, MutationOp,
    PolicyDecision, Revision,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Seeds
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct AdversarialSeeds {
    poisoning: u64,
    cross_tenant_leak: u64,
    replay: u64,
    push_throughput: u64,
    pull_latency: u64,
    audit_persistence: u64,
}

fn load_seeds() -> AdversarialSeeds {
    let contents = std::fs::read_to_string("benches/fixtures/adversarial_seeds.json")
        .expect("failed to read seeds file");
    serde_json::from_str(&contents).expect("failed to parse seeds file")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn temp_workspace(prefix: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("rango-adversarial-bench-{prefix}-{pid}-{n}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn make_mutation(tenant_id: &str, namespace: &str, write_id: &str, trust_score: f64) -> Mutation {
    let doc_id = DocumentId::new_uuid_v7();
    let rev = Revision::now("bench");
    Mutation {
        op: MutationOp::Insert,
        collection: "state".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(doc! { "payload": write_id }),
        seq: 0,
        timestamp: bson::DateTime::now(),
        rev: rev.clone(),
        write_id: write_id.to_string(),
        metadata: MutationMetadata {
            id: doc_id.clone(),
            namespace: namespace.to_string(),
            tenant_id: tenant_id.to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "bench".to_string(),
            actor: "bench".to_string(),
            lineage: doc_id.to_string(),
            schema_version: 1,
            trust_score,
            verified: Some(true),
            expires_at: None,
        },
    }
}

// ---------------------------------------------------------------------------
// 1. Poisoning rejection latency
// ---------------------------------------------------------------------------

fn poisoning_rejection_latency(c: &mut Criterion) {
    let seeds = load_seeds();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seeds.poisoning);

    let control_plane = ControlPlane::default();
    let ctx = WriteContext {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        actor: "bench".to_string(),
        source: "bench".to_string(),
        tier: MemoryTier::State,
    };

    let mut group = c.benchmark_group("poisoning_rejection_latency");
    for _ in 0..5 {
        let trust_score = rng.gen_range(0.0..0.25);
        let payload = WritePayload::StateWithTrust {
            document: doc! { "data": "test" },
            trust_score,
        };
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{:.2}", trust_score)),
            &payload,
            |b, p| {
                b.iter(|| {
                    black_box(control_plane.write_path(&ctx, p).unwrap());
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// 2. Cross-tenant leak fuzzer
// ---------------------------------------------------------------------------

fn cross_tenant_leak_check(c: &mut Criterion) {
    let seeds = load_seeds();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seeds.cross_tenant_leak);

    let workspace = temp_workspace("leak");
    let oplog = Arc::new(FileOplog::new(workspace.join("oplog.rgo")).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "node-a", "tenant-a");
    state.add_token_with_tenant("token-b", "node-b", "tenant-b");

    let principal_a = AuthPrincipal {
        node_id: "node-a".to_string(),
        tenant_id: "tenant-a".to_string(),
    };

    // Generate deterministic mutations for tenant-a
    let n = 100usize;
    for i in 0..n {
        let write_id = format!("leak-{}-{}", i, rng.r#gen::<u64>());
        let req = PushRequest {
            node_id: "node-a".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns".to_string(),
            mutations: vec![make_mutation("tenant-a", "ns", &write_id, 0.95)],
            last_checkpoint: Checkpoint::initial(),
        };
        let resp = state.process_push(req, principal_a.clone()).unwrap();
        assert!(
            !resp.accepted_seqs.is_empty(),
            "mutation {i} should be accepted"
        );
    }

    let principal_b = AuthPrincipal {
        node_id: "node-b".to_string(),
        tenant_id: "tenant-b".to_string(),
    };
    let pull_req = PullRequest {
        node_id: "node-b".to_string(),
        tenant_id: "tenant-b".to_string(),
        namespace: "ns".to_string(),
        since_checkpoint: Checkpoint::initial(),
    };

    c.bench_function("cross_tenant_leak_check", |b| {
        b.iter(|| {
            let resp = state
                .process_pull(pull_req.clone(), principal_b.clone(), MemoryTier::State)
                .unwrap();
            assert_eq!(resp.mutations.len(), 0, "tenant-b must observe 0 mutations");
            black_box(resp);
        });
    });
}

// ---------------------------------------------------------------------------
// 3. Replay determinism
// ---------------------------------------------------------------------------

fn replay_determinism(c: &mut Criterion) {
    let seeds = load_seeds();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seeds.replay);

    // Setup: populate oplog A
    let workspace = temp_workspace("replay");
    let oplog_a = Arc::new(FileOplog::new(workspace.join("oplog-a.rgo")).unwrap());
    let state_a = Arc::new(ServerState::new(oplog_a.clone()));
    state_a.add_token_with_tenant("token-a", "node-a", "tenant-a");

    let principal_a = AuthPrincipal {
        node_id: "node-a".to_string(),
        tenant_id: "tenant-a".to_string(),
    };

    let n = 50usize;
    for i in 0..n {
        let write_id = format!("replay-{}-{}", i, rng.r#gen::<u64>());
        let req = PushRequest {
            node_id: "node-a".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns".to_string(),
            mutations: vec![make_mutation("tenant-a", "ns", &write_id, 0.95)],
            last_checkpoint: Checkpoint::initial(),
        };
        state_a.process_push(req, principal_a.clone()).unwrap();
    }

    let checkpoint_a = scoped_latest_checkpoint(&state_a, "tenant-a", "ns").unwrap();
    let entries = oplog_a.read_since(1, 1000).unwrap();

    // Pre-compute replay path so each iteration starts from a clean file
    let replay_path = workspace.join("oplog-replay.rgo");

    c.bench_function("replay_determinism", |b| {
        b.iter(|| {
            // Clean up previous replay file if it exists
            if replay_path.exists() {
                std::fs::remove_file(&replay_path).unwrap();
            }

            let oplog_b = Arc::new(FileOplog::new(&replay_path).unwrap());
            for entry in &entries {
                oplog_b.append(entry.clone()).unwrap();
            }

            let state_b = Arc::new(ServerState::new(oplog_b));
            let checkpoint_b = scoped_latest_checkpoint(&state_b, "tenant-a", "ns").unwrap();

            assert_eq!(
                checkpoint_a, checkpoint_b,
                "checkpoints must converge after replay"
            );
            black_box(checkpoint_b);
        });
    });
}

// ---------------------------------------------------------------------------
// 4. Push throughput
// ---------------------------------------------------------------------------

fn push_throughput(c: &mut Criterion) {
    let seeds = load_seeds();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seeds.push_throughput);

    let workspace = temp_workspace("push");
    let oplog = Arc::new(FileOplog::new(workspace.join("oplog.rgo")).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "node-a", "tenant-a");

    let principal = AuthPrincipal {
        node_id: "node-a".to_string(),
        tenant_id: "tenant-a".to_string(),
    };

    let mut counter = 0u64;

    c.bench_function("push_throughput", |b| {
        b.iter(|| {
            counter += 1;
            let write_id = format!("push-{}-{}", counter, rng.r#gen::<u64>());
            let req = PushRequest {
                node_id: "node-a".to_string(),
                tenant_id: "tenant-a".to_string(),
                namespace: "ns".to_string(),
                mutations: vec![make_mutation("tenant-a", "ns", &write_id, 0.95)],
                last_checkpoint: Checkpoint::initial(),
            };
            black_box(state.process_push(req, principal.clone()).unwrap());
        });
    });
}

// ---------------------------------------------------------------------------
// 5. Pull latency
// ---------------------------------------------------------------------------

fn pull_latency(c: &mut Criterion) {
    let seeds = load_seeds();
    let mut rng = rand::rngs::StdRng::seed_from_u64(seeds.pull_latency);

    let workspace = temp_workspace("pull");
    let oplog = Arc::new(FileOplog::new(workspace.join("oplog.rgo")).unwrap());
    let state = Arc::new(ServerState::new(oplog));
    state.add_token_with_tenant("token-a", "node-a", "tenant-a");

    let principal = AuthPrincipal {
        node_id: "node-a".to_string(),
        tenant_id: "tenant-a".to_string(),
    };

    let n = 100usize;
    for i in 0..n {
        let write_id = format!("pull-{}-{}", i, rng.r#gen::<u64>());
        let req = PushRequest {
            node_id: "node-a".to_string(),
            tenant_id: "tenant-a".to_string(),
            namespace: "ns".to_string(),
            mutations: vec![make_mutation("tenant-a", "ns", &write_id, 0.95)],
            last_checkpoint: Checkpoint::initial(),
        };
        state.process_push(req, principal.clone()).unwrap();
    }

    let pull_req = PullRequest {
        node_id: "node-a".to_string(),
        tenant_id: "tenant-a".to_string(),
        namespace: "ns".to_string(),
        since_checkpoint: Checkpoint::initial(),
    };

    c.bench_function("pull_latency", |b| {
        b.iter(|| {
            black_box(
                state
                    .process_pull(pull_req.clone(), principal.clone(), MemoryTier::State)
                    .unwrap(),
            );
        });
    });
}

// ---------------------------------------------------------------------------
// 6. Audit persistence
// ---------------------------------------------------------------------------

fn audit_persistence(c: &mut Criterion) {
    let _seeds = load_seeds();

    let workspace = temp_workspace("audit");
    let oplog = Arc::new(FileOplog::new(workspace.join("oplog.rgo")).unwrap());
    let state = Arc::new(ServerState::new(oplog));

    let decision = GovernanceDecision {
        decision: PolicyDecision::Allow,
        reason: "benchmark".to_string(),
    };

    c.bench_function("audit_persistence", |b| {
        b.iter(|| {
            black_box(
                state
                    .persist_audit_evidence("write", "tenant-a", "ns", None, &decision)
                    .unwrap(),
            );
        });
    });
}

// ---------------------------------------------------------------------------
// Criterion entrypoint
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    poisoning_rejection_latency,
    cross_tenant_leak_check,
    replay_determinism,
    push_throughput,
    pull_latency,
    audit_persistence
);
criterion_main!(benches);
