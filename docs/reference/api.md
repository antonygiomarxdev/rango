# Rango API Reference

## SDK (`rango-sdk`)

### `RangoClient`

The main entry point for applications.

```rust
use std::sync::Arc;
use rango_storage::MemoryStorage;
use rango_oplog::NullOplog;
use rango_sdk::RangoClient;

let storage = Arc::new(MemoryStorage::new());
let oplog = Arc::new(NullOplog::new());
let client = RangoClient::open(storage, oplog, "my-node")?;
```

### `CollectionClient`

Operations on a single collection.

```rust
let users = client.collection("users");

// Insert
let id = users.insert_one(doc! { "name": "Alice", "age": 30 })?;

// Find by ID
let doc = users.find_one(&id)?;

// Find many
let cursor = users.find_many()?;

// Update
users.update_one(&id, doc! { "$set": { "age": 31 } })?;

// Delete (tombstone)
users.delete_one(&id)?;
```

### Advanced Queries

```rust
use rango_sdk::RangoClient;

// Find with filter
let cursor = client.engine.find(
    &CollectionName::new("users"),
    &doc! { "age": { "$gte": 18 } },
    None, None, None, None
)?;

// Find with projection (include only name)
let cursor = client.engine.find(
    &CollectionName::new("users"),
    &doc! {},
    Some(&doc! { "name": 1 }),
    None, None, None
)?;

// Find with sort and limit
let cursor = client.engine.find(
    &CollectionName::new("users"),
    &doc! {},
    None,
    Some(("age", false)), // descending
    None,
    Some(10)
)?;
```

### Migration

```rust
// Import from JSON Lines
let result = client.import_json("users", "users.json", &ConsoleProgress)?;
println!("Imported {} documents", result.imported);

// Export to JSON Lines
let result = client.export_json("users", "users-export.json")?;
println!("Exported {} documents", result.exported);
```

### Sync

```rust
use rango_sync::client::SyncClient;
use rango_sync::scheduler::SyncScheduler;

let sync_client = SyncClient::new("http://server:8080", "my-token");
let scheduler = SyncScheduler::default();

let result = scheduler.run_once(
    "my-node",
    &queue,
    &oplog,
    &checkpoint_store,
    &sync_client
).await?;

println!("Pushed: {}, Pulled: {}", result.pushed, result.pulled);
```

### Metrics

```rust
let metrics = client.engine.metrics().snapshot();
println!("Inserts: {}", metrics.inserts);
println!("Finds: {}", metrics.finds);
println!("Updates: {}", metrics.updates);
println!("Deletes: {}", metrics.deletes);
println!("Sync pushes: {}", metrics.sync_pushes);
println!("Sync pulls: {}", metrics.sync_pulls);
```

## CLI

```bash
# Initialize local memory workspace
rango init ./memory-home

# Import documents
rango import --collection users users.json

# Export documents
rango export --collection users --output users.json

# Run benchmarks
rango bench --count 10000

# Diagnostics
rango doctor ./memory-home

# Sync with remote server
rango sync ./memory-home --server http://localhost:8080 --token my-secret-token
```

## BSON Types

Rango uses BSON natively. When importing from Extended JSON/BSON representations:

| Extended JSON | BSON Type |
|---------------|-----------|
| `{"$oid": "..."}` | ObjectId |
| `{"$date": "..."}` | DateTime |
| `{"$numberInt": "..."}` | Int32 |
| `{"$numberLong": "..."}` | Int64 |
| `{"$numberDouble": "..."}` | Double |

## Reserved Fields

The following fields are managed by Rango and should not be modified directly:

- `_id` â€” Document identifier (UUID v7 or preserved ObjectId)
- `_rev` â€” Hybrid Logical Clock revision string
- `_updated_at` â€” Last modification timestamp
- `_source_node` â€” Node that made the last modification
- `_deleted` â€” Tombstone flag
- `_conflicts` â€” Array of conflicting versions

