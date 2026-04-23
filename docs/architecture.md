# Rango Architecture

## Crate Structure
```
types → core, storage, index, query, oplog, sync, server, sdk-rust, cli
storage → index, query, core
index → query, core
query → core
oplog → sync, core, server
sync → server, core
core → sdk-rust, cli, server
sdk-rust → cli
```

## Key Abstractions
- `StorageEngine`: trait base KV + trait separado para transacciones.
- `DocumentId`: BSON nativo (UUID v7 o ObjectId).
- `Mutation`: operación de sync (insert/update/delete).
- `Revision`: monotonic counter para LWW conflict resolution.
