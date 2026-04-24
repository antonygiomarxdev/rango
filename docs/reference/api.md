# Rango API Reference

## SDK (`rango-sdk`)

### Open a client with durable local storage

```rust
use std::sync::Arc;

use rango_oplog::FileOplog;
use rango_sdk::RangoClient;
use rango_storage::RedbStorage;

let root = std::path::PathBuf::from("./memory");
std::fs::create_dir_all(&root)?;

let storage = Arc::new(RedbStorage::open(root.join("data.redb"))?);
let oplog = Arc::new(FileOplog::new(root.join("oplog.rgo"))?);
let client = RangoClient::open(storage, oplog, "node-1")?;
```

### Collection operations

```rust
use bson::doc;

let users = client.collection("users");

let id = users.insert_one(doc! { "name": "Alice", "age": 30 })?;
let one = users.find_one(&id)?;
let cursor = users.find_many()?;

users.update_one(&id, doc! { "$set": { "age": 31 } })?;
users.delete_one(&id)?; // tombstone delete
```

### Query engine access

```rust
use bson::doc;
use rango_types::CollectionName;

let cursor = client.engine.find(
    &CollectionName::new("users"),
    &doc! { "age": { "$gte": 18 } },
    None,
    None,
    None,
    Some(100),
)?;
```

### Import/export JSON Lines

```rust
use rango_sdk::migrate::ConsoleProgress;

let import_result = client.import_json("users", "users.jsonl", &ConsoleProgress)?;
let export_result = client.export_json("users", "users-export.jsonl")?;
```

### Metrics

```rust
let metrics = client.engine.metrics().snapshot();
println!("inserts={}", metrics.inserts);
println!("finds={}", metrics.finds);
println!("updates={}", metrics.updates);
println!("deletes={}", metrics.deletes);
```

## CLI (`rango`)

```bash
rango init ./memory
rango inspect ./memory
rango import --path ./memory --collection events ./events.jsonl
rango export --path ./memory --collection events --output ./events-export.jsonl
rango doctor ./memory
rango sync ./memory --server http://localhost:8080 --token dev-token --node-id node-a
```

## Server (`rango-server`)

```bash
rango-server --bind 0.0.0.0 --port 8080 --token dev-token --oplog-path ./server-oplog.rgo
```

Environment overrides are also supported:

- `RANGO_BIND`
- `RANGO_PORT`
- `RANGO_TOKEN`
- `RANGO_OPLOG_PATH`

## Reserved document fields

Rango controls these fields internally:

- `_id`
- `_rev`
- `_updated_at`
- `_source_node`
- `_deleted`
- `_conflicts`
