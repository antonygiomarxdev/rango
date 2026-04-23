# Rango

<p align="center">
  <strong>Durable document memory for stateful AI systems.</strong><br>
  Local-first state · Durable history · Incremental sync
</p>

<p align="center">
  <a href="https://github.com/antonygiomarxdev/rango/actions/workflows/ci.yml"><img src="https://github.com/antonygiomarxdev/rango/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/antonygiomarxdev/rango/releases"><img src="https://img.shields.io/github/v/release/antonygiomarxdev/rango" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-BSL%201.1%20%2B%20commercial-blue.svg" alt="License"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg" alt="MSRV"></a>
</p>

---

## Why Rango?

Stateful agents, copilots, and long-running automations all hit the same wall: they need memory that survives restarts, survives disconnects, and syncs without replaying the whole world. Rango is built around that reality.

| Problem | Rango's answer |
|---------|---------------|
| Agent process restarts or crashes | State and change history are stored durably |
| Memory must keep working offline | Local-first reads and writes, sync later |
| Multiple nodes continue from diverged state | Incremental sync with `_rev` ordering and conflict retention |
| Memory should look like documents, not rows | BSON-native documents and document-level mutations |
| Platform teams need safe operational state | Encryption at rest, oplog, checkpoints, and audit-friendly history |

Rango is **not** a MongoDB clone, not SQLite with JSON bolted on, and not an analytics database. It is a primitive you embed directly in your application to give AI systems durable operational memory.

---

## Core Capabilities

- **Durable document memory** — documents, revisions, and change history are first-class
- **Local-first runtime state** — no network dependency for the hot path
- **Incremental sync** — checkpoint-based push/pull over HTTP/JSON
- **Conflict retention** — Last-Write-Wins plus `_conflicts` history for reconciliation
- **Document query and mutation engine** — `$eq`, `$in`, `$gt`, `$gte`, `$lt`, `$lte`, `$and`, `$or`, plus `$set`/`$unset`/`$inc`
- **Secondary indexes** — B-tree indexes created and dropped at runtime
- **At-rest encryption** — AES-256-GCM with passphrase-derived keys
- **Import/export** — JSON Lines and MongoDB Extended JSON migration paths
- **Operational tooling** — `init`, `inspect`, `import`, `export`, `bench`, `doctor`, `sync`
- **Observability** — structured tracing, metrics, and durable sync metadata

---

## Quick Start

### As a library

```toml
# Cargo.toml
[dependencies]
rango-sdk = { git = "https://github.com/antonygiomarxdev/rango", package = "rango-sdk" }
tokio = { version = "1", features = ["full"] }
```

```rust
use rango_sdk::{RangoClient, RangoConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = RangoClient::open("./my-data", RangoConfig::default()).await?;
    let memory = client.collection("agent_memory");

    // Persist a durable memory record
    let id = memory.insert_one(json!({
        "agent_id": "planner-01",
        "kind": "conversation_summary",
        "content": "User wants a phased rollout with strong safety guarantees.",
        "importance": 0.87
    })).await?;

    // Recover that memory later
    let doc = memory.find_one(json!({ "_id": id })).await?;
    println!("{doc:?}");

    // Query operational memory by semantic metadata
    let important = memory.find_many(json!({ "importance": { "$gte": 0.8 } })).await?;
    println!("Important memories: {}", important.len());

    Ok(())
}
```

### As a sync server

```bash
cargo build --release -p rango-server
RANGO_TOKEN=secret ./target/release/rango-server --port 8080 --data ./server-data
```

### CLI

```bash
cargo install --git https://github.com/antonygiomarxdev/rango --package rango-cli

rango init ./my-data
rango import --collection events ./dump.jsonl
rango inspect ./my-data
rango sync --server http://localhost:8080 --token secret ./my-data
rango doctor ./my-data
```

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    rango-sdk (public API)                │
├──────────────────────────┬──────────────────────────────┤
│      rango-core          │       rango-server            │
│  (engine orchestration)  │   (Axum HTTP push/pull)       │
├────────┬─────────┬───────┴────────┬────────────────────-┤
│ query  │  index  │    oplog       │       sync           │
├────────┴─────────┴────────────────┴─────────────────────┤
│                    rango-storage                         │
│          (StorageEngine trait + in-memory impl)          │
├─────────────────────────────────────────────────────────┤
│                    rango-types                           │
│    (DocumentId · Revision · Mutation · Checkpoint)       │
└─────────────────────────────────────────────────────────┘
```

Every layer depends only on the layer below it. The `StorageEngine` trait is the main extension point, and the core abstraction is not just storage: it is durable document state plus revisioned history.

Full architecture: [docs/architecture.md](docs/architecture.md)

---

## Crates

| Crate | Description |
|-------|-------------|
| `rango-types` | Shared primitives for document identity, revisions, mutations, and checkpoints |
| `rango-storage` | Pluggable KV engine, durability primitives, and AES-256-GCM encryption |
| `rango-index` | Primary and secondary index management |
| `rango-query` | Filter, projection, sort, and document update operators |
| `rango-oplog` | Durable append-only change history |
| `rango-sync` | Incremental sync engine, checkpoints, and conflict handling |
| `rango-core` | Main engine for document state, metadata, and orchestration |
| `rango-server` | Axum-based HTTP sync server |
| `rango-sdk` | Public Rust SDK for embedding Rango into applications |
| `rango-cli` | Operator tooling for local stores and sync flows |

---

## Target Use Cases

- durable memory for copilots and autonomous agents
- local operational state for automations that must survive restarts
- syncable document state across desktop, edge, and backend runtimes
- embedded memory layers where full database infrastructure would be overkill

---

## Sync Protocol

Rango uses a simple checkpoint-based push/pull protocol over HTTP/JSON.

```
Edge Node                          Sync Server
    |                                   |
    |-- POST /push (mutations) -------> |
    |<- 200 OK (acked seq numbers) -----|
    |                                   |
    |-- GET /pull?since=<checkpoint> -> |
    |<- 200 OK (mutations) -------------|
```

Each document carries a `_rev` (Hybrid Logical Clock timestamp). Conflicts are resolved Last-Write-Wins; the losing version is stored in `_conflicts` (max 10 retained), which makes sync suitable for operational memory instead of stateless cache replication.

Full spec: [docs/sync-protocol.md](docs/sync-protocol.md)

---

## Performance

Benchmarks run on a single-core ARM Cortex-A53 (Raspberry Pi 4):

| Operation | p50 | p99 |
|-----------|-----|-----|
| `find_one` by `_id` | < 1 ms | < 2 ms |
| `insert_one` (no sync) | < 2 ms | < 5 ms |
| `find_many` (1 k docs, no index) | < 5 ms | < 10 ms |
| `find_many` (1 k docs, indexed) | < 1 ms | < 3 ms |

Run benchmarks yourself:
```bash
cargo bench --workspace
```

---

## Documentation

- [Vision & Principles](docs/vision.md)
- [Architecture](docs/architecture.md)
- [API Reference](docs/api.md)
- [Query Language](docs/query-language.md)
- [Sync Protocol](docs/sync-protocol.md)
- [Migration Guide](docs/migration.md)
- [Security](docs/SECURITY.md)
- [ADR-001: Storage Engine](docs/adr/ADR-001-storage-engine.md)
- [ADR-002: ID Generation](docs/adr/ADR-002-id-generation.md)
- [ADR-003: Sync Protocol](docs/adr/ADR-003-sync-protocol.md)

---

## MSRV Policy

Rango maintains a minimum supported Rust version of **1.85** (Rust Edition 2024).
MSRV bumps require a minor version bump and are announced in the changelog.

---

## Licensing

Rango is **source-available**, not OSI open source.

The repository is licensed under **Business Source License 1.1** with a project-specific
Additional Use Grant designed to keep the code accessible to builders and smaller teams,
while requiring a paid commercial license for large organizations and monetized platform use.

You can generally use Rango without a separate commercial license for:

- personal, educational, research, and evaluation use
- open-source projects and non-profits
- internal self-hosted use by organizations below the commercial threshold
- small teams with fewer than 25 employees and less than USD 2,000,000 annual revenue

A separate commercial license is required for:

- hosted or managed services offered to third parties
- embedded commercial products distributed to third parties
- organizations at or above either commercial threshold
- cloud providers, hyperscalers, and competitive offerings

See [LICENSE](LICENSE) for the binding terms and [LICENSING.md](LICENSING.md) for the practical usage matrix.

---

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a PR.
Bug reports, feature requests, and discussions go in [GitHub Issues](https://github.com/antonygiomarxdev/rango/issues) and [Discussions](https://github.com/antonygiomarxdev/rango/discussions).

---

## License

Source-available under [Business Source License 1.1](LICENSE) with a project-specific Additional Use Grant.

Commercial licenses are available for hosted, embedded, large-enterprise, and competitive uses.
See [LICENSING.md](LICENSING.md) for the practical policy.
