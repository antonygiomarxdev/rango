# Rango

Durable memory substrate for stateful AI systems.

Rango is memory-first, document-native, and local-first. It is not a generic database product and not a workflow product.

## Product Position

Rango is built as infrastructure below agents and apps:

- canonical state and history for operational memory
- append-only events plus materialized current state
- local-first writes with incremental sync
- clear layer boundaries for future semantic/retrieval projections

## Version Scope

### v0.1.0 — Durable Substrate

- embedded local engine with `redb` backend
- append-only oplog + deterministic replay
- current state store with snapshots/checkpoints
- simple B-tree indexes
- sync foundations (push/pull + checkpoint progression)
- tenant/namespace isolation primitives

### v0.2.0 — Security & Governance

- audit trail completeness (`__governance_audit`)
- anomaly detection + containment gates
- graceful degradation (`DegradingStorage`)
- OpenTelemetry metrics + health endpoints
- Python (PyO3) and TypeScript/Node.js (napi-rs) bindings
- adversarial benchmarks + CLI audit report

### v0.3.0 — Operability & Sync (in progress)

- multi-node sync hardening (partition/heal regression suite)
- backup/restore CLI with optional S3 sink
- external read contract (how tools read from Rango)
- trust-aware ranking input contract documentation

Out of core (post-v0.3.0):

- vector-native retrieval (requires ADR on embedding versioning)
- graph reasoning engine
- workflow/orchestration semantics
- dynamic plugin loading in core

## Install

### Option 1: GitHub Releases (recommended for product teams)

Download the latest platform archive from [GitHub Releases](https://github.com/antonygiomarxdev/rango/releases), then add the extracted binaries to your `PATH`:

- `rango` (CLI)
- `rango-server` (sync hub)

### Option 2: Build/install from source

```bash
cargo install --path crates/cli
cargo install --path crates/server
```

### Option 3: Run directly from workspace

```bash
cargo run -p rango-cli -- --help
cargo run -p rango-server -- --help
```

## Quick Start

### 1) Local memory workspace via CLI

```bash
rango init ./memory
rango inspect ./memory
```

Import/export test:

```bash
rango import --path ./memory --collection events ./events.jsonl
rango export --path ./memory --collection events --output ./events-export.jsonl
rango doctor ./memory
```

### 2) Embed as a Rust library

```rust
use std::sync::Arc;

use bson::doc;
use rango_oplog::FileOplog;
use rango_sdk::RangoClient;
use rango_storage::RedbStorage;

fn main() -> anyhow::Result<()> {
    let root = std::path::PathBuf::from("./memory");
    std::fs::create_dir_all(&root)?;

    let storage = Arc::new(RedbStorage::open(root.join("data.redb"))?);
    let oplog = Arc::new(FileOplog::new(root.join("oplog.rgo"))?);
    let client = RangoClient::open(storage, oplog, "agent-node-1")?;

    let state = client.collection("agent_state");
    let id = state.insert_one(doc! {
        "tenant_id": "acme",
        "namespace": "planner",
        "status": "running",
        "step": "collect-context"
    })?;

    let current = state.find_one(&id)?;
    println!("current={current:?}");

    Ok(())
}
```

### 3) Run a sync hub

Start server:

```bash
rango-server --bind 0.0.0.0 --port 8080 --token dev-token --oplog-path ./server-oplog.rgo
```

Sync a workspace:

```bash
rango sync ./memory --server http://localhost:8080 --token dev-token --node-id node-a
```

## Integration Modes for Products Built on Rango

1. **Embedded mode**: each product instance embeds Rango locally for offline durability.
2. **Hub-and-spoke mode**: product instances sync to a central `rango-server` hub.
3. **Multi-product mode**: each product gets tenant/namespace isolation and shared sync topology.

This allows products other than OpenClaw to use the same substrate with the same core contract.

## Language Bindings

- **Rust**: `rango-sdk` (native crate)
- **Python**: `pip install rango` (PyO3 + maturin, see `crates/python/`)
- **TypeScript/Node.js**: `npm install rango` (napi-rs, see `crates/node/`)

## Integrations

- OpenClaw: [integration guide](docs/integrations/openclaw.md) + [memory contract](docs/integrations/openclaw-memory-contract.json)
- Generic product integration pattern: [docs index](docs/README.md)

## Release Naming

- **Stable tags**: `vX.Y.Z` → GitHub release name `Rango vX.Y.Z`
- **Main prereleases**: `vX.Y.Z-rango-YYYYMMDD-HHMM-SHA` → GitHub release name `Rango vX.Y.Z-rango.YYYYMMDD-HHMM+SHA`
- **Channel tags**:
  - stable releases move `latest`
  - prereleases from `main` move `nightly` and `beta`

## Rango Integration Skill

A [Claude Code skill](skills/rango-integration/SKILL.md) is available for agentic development on Rango. It provides four modes:

- **Setup**: Scaffold a new Rango workspace or integration
- **Pattern**: Apply tested Rango patterns (sync, audit, tenant isolation)
- **Architecture**: Validate changes against Rango's design constraints
- **Audit**: Review code for Rango-specific correctness and safety

## Validation

Run full checks:

```bash
cargo check --workspace
cargo test --workspace
```

Run end-to-end smoke script:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/smoke-e2e.ps1
```

## Workspace Crates

- `rango-types`: shared canonical types
- `rango-core`: engine + deterministic apply/replay
- `rango-storage`: storage engine trait + `redb` backend + crypto
- `rango-oplog`: append-only operation log
- `rango-sync`: sync queue/checkpoint/scheduler/client
- `rango-server`: HTTP sync hub
- `rango-sdk`: embeddable Rust API
- `rango-cli`: operator and local tooling

## Documentation

- [Docs Index](docs/README.md)
- [Roadmap](ROADMAP.md)
- [Vision](docs/vision.md)
- [Architecture Overview](docs/architecture/overview.md)
- [Memory Model](docs/architecture/memory/model.md)
- [API Reference](docs/reference/api.md)
- [Sync Protocol](docs/reference/sync-protocol.md)
- [OpenClaw Integration Guide](docs/integrations/openclaw.md)
- [Work Management Workflow](docs/operations/workflow.md)
- [Security](docs/operations/security.md)
- [ADRs](docs/adr/)

## License

Source-available under [Business Source License 1.1](LICENSE) with project-specific additional use grant.
See [LICENSING.md](LICENSING.md) for practical policy.
