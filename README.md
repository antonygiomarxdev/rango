# Rango

<p align="center">
  <strong>Local-first embedded document database for edge devices and IoT gateways.</strong><br>
  Sub-millisecond reads · Offline writes · Reliable incremental sync
</p>

<p align="center">
  <a href="https://github.com/antonygiomarxdev/rango/actions/workflows/ci.yml"><img src="https://github.com/antonygiomarxdev/rango/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/antonygiomarxdev/rango/releases"><img src="https://img.shields.io/github/v/release/antonygiomarxdev/rango" alt="Release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-BSL%201.1%20%2B%20commercial-blue.svg" alt="License"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg" alt="MSRV"></a>
</p>

---

## Why Rango?

Field operations, IoT gateways, and edge deployments share a common constraint: **the network cannot be trusted**. Rango is built around that reality.

| Problem | Rango's answer |
|---------|---------------|
| Network goes down mid-operation | Writes are persisted locally first, synced later |
| Multiple devices edit the same document | Last-Write-Wins by `_rev` (Hybrid Logical Clock), conflicts preserved in `_conflicts` |
| Data must survive a power cut | Append-only oplog + WAL-based crash recovery |
| Data at rest must be encrypted | AES-256-GCM on every byte, PBKDF2-SHA256 key derivation |
| Team knows MongoDB API | BSON-native, familiar CRUD and query operators |

Rango is **not** a distributed database, a full MongoDB replacement, or a cloud service. It is a primitive you embed directly in your application — like SQLite, but for documents.

---

## Features

- **Embedded & local-first** — no server required for reads or writes
- **BSON-native** — documents are BSON; IDs are UUID v7 (monotonic, sortable)
- **MongoDB-compatible query API** — `$eq`, `$in`, `$gt`, `$gte`, `$lt`, `$lte`, `$and`, `$or`
- **Update operators** — `$set`, `$unset`, `$inc`
- **Secondary indexes** — B-tree, create/drop at runtime
- **Incremental sync** — checkpoint-based push/pull over HTTP/JSON
- **Conflict resolution** — Last-Write-Wins with full version history (`_conflicts`)
- **At-rest encryption** — AES-256-GCM, passphrase-based key derivation
- **Import/export** — JSON Lines and MongoDB Extended JSON
- **CLI tools** — `init`, `inspect`, `import`, `export`, `bench`, `doctor`, `sync`
- **Observability** — structured tracing, counters for every operation

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
    let sensors = client.collection("sensors");

    // Insert
    let id = sensors.insert_one(json!({
        "device_id": "gw-001",
        "temp_c": 23.5,
        "humidity_pct": 61
    })).await?;

    // Find
    let doc = sensors.find_one(json!({ "_id": id })).await?;
    println!("{doc:?}");

    // Query
    let hot = sensors.find_many(json!({ "temp_c": { "$gt": 30 } })).await?;
    println!("Hot sensors: {}", hot.len());

    Ok(())
}
```

### As a server (sync target)

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

Every layer depends only on the layer below it. The `StorageEngine` trait is the main extension point — plug in `sled`, `redb`, `fjall`, or any other KV store.

Full architecture: [docs/architecture.md](docs/architecture.md)

---

## Crates

| Crate | Description |
|-------|-------------|
| `rango-types` | Shared primitives: DocumentId, Revision, Mutation, Checkpoint |
| `rango-storage` | Pluggable KV engine + AES-256-GCM encryption |
| `rango-index` | Primary and secondary index management |
| `rango-query` | Filter, projection, sort, update operators |
| `rango-oplog` | Append-only operation log with compaction |
| `rango-sync` | Incremental sync engine + conflict resolution |
| `rango-core` | Engine orchestrating all subsystems |
| `rango-server` | Axum-based HTTP sync server |
| `rango-sdk` | Public Rust SDK |
| `rango-cli` | CLI tool |

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

Each document carries a `_rev` (Hybrid Logical Clock timestamp). Conflicts are resolved Last-Write-Wins; the losing version is stored in `_conflicts` (max 10 retained).

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
