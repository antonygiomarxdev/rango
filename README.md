# Rango

<p align="center">
  <strong>Local-first embedded document database for edge devices and IoT gateways.</strong><br>
  Sub-millisecond reads · Offline writes · Reliable incremental sync
</p>

<p align="center">
  <a href="https://github.com/antonygiomarxdev/rango/actions/workflows/ci.yml"><img src="https://github.com/antonygiomarxdev/rango/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/rango-core"><img src="https://img.shields.io/crates/v/rango-core.svg" alt="Crates.io"></a>
  <a href="https://docs.rs/rango-core"><img src="https://docs.rs/rango-core/badge.svg" alt="docs.rs"></a>
  <a href="LICENSE-MIT"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg" alt="License"></a>
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
rango-sdk = "0.1"
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
cargo install rango-cli

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

| Crate | Description | Crates.io |
|-------|-------------|-----------|
| `rango-types` | Shared primitives: DocumentId, Revision, Mutation, Checkpoint | [![](https://img.shields.io/crates/v/rango-types.svg)](https://crates.io/crates/rango-types) |
| `rango-storage` | Pluggable KV engine + AES-256-GCM encryption | [![](https://img.shields.io/crates/v/rango-storage.svg)](https://crates.io/crates/rango-storage) |
| `rango-index` | Primary and secondary index management | [![](https://img.shields.io/crates/v/rango-index.svg)](https://crates.io/crates/rango-index) |
| `rango-query` | Filter, projection, sort, update operators | [![](https://img.shields.io/crates/v/rango-query.svg)](https://crates.io/crates/rango-query) |
| `rango-oplog` | Append-only operation log with compaction | [![](https://img.shields.io/crates/v/rango-oplog.svg)](https://crates.io/crates/rango-oplog) |
| `rango-sync` | Incremental sync engine + conflict resolution | [![](https://img.shields.io/crates/v/rango-sync.svg)](https://crates.io/crates/rango-sync) |
| `rango-core` | Engine orchestrating all subsystems | [![](https://img.shields.io/crates/v/rango-core.svg)](https://crates.io/crates/rango-core) |
| `rango-server` | Axum-based HTTP sync server | [![](https://img.shields.io/crates/v/rango-server.svg)](https://crates.io/crates/rango-server) |
| `rango-sdk` | Public Rust SDK | [![](https://img.shields.io/crates/v/rango-sdk.svg)](https://crates.io/crates/rango-sdk) |
| `rango` | CLI tool | [![](https://img.shields.io/crates/v/rango.svg)](https://crates.io/crates/rango) |

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

## Contributing

Contributions are welcome! Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a PR.
Bug reports, feature requests, and discussions go in [GitHub Issues](https://github.com/antonygiomarxdev/rango/issues) and [Discussions](https://github.com/antonygiomarxdev/rango/discussions).

---

## License

Dual-licensed under your choice of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in Rango shall be dual-licensed as above, without any additional terms or conditions.
