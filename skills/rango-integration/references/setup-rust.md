# Rango Setup — Rust

## Dependencies

Add to `Cargo.toml`:
```toml
[dependencies]
rango-sdk = "0.2"
rango-types = "0.2"
rango-storage = "0.2"
rango-oplog = "0.2"
```

## Basic Initialization

```rust
use std::sync::Arc;
use rango_sdk::RangoClient;
use rango_storage::{DegradingStorage, RedbStorage};
use rango_oplog::FileOplog;

fn open_workspace(path: &std::path::Path) -> Result<RangoClient, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(path)?;
    
    let storage_path = path.join("data.redb");
    let inner_storage = RedbStorage::open(&storage_path)?;
    let storage = Arc::new(
        DegradingStorage::with_default_threshold(inner_storage, &storage_path)?
    );
    
    let oplog_path = path.join("oplog.bin");
    let oplog = Arc::new(FileOplog::new(&oplog_path)?);
    
    let client = RangoClient::open(storage, oplog, "my-app")?;
    Ok(client)
}
```

## Next Steps

- Use Pattern Generator to persist specific state
- See `../examples/session-memory.rs` for complete example
