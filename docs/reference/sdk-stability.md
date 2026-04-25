# Rango Rust SDK Stability Policy (v0.1.0+)

This document defines the stability guarantees and forward-compatibility commitments for the Rango Rust SDK (`rango-sdk`). External products integrating the SDK must follow the breaking-change process described below.

---

## SemVer Rules

The Rango Rust SDK follows [Semantic Versioning](https://semver.org/):

- **MAJOR** version increments when breaking changes are released (after deprecation period).
- **MINOR** version increments for new stable features and experimental/unstable items.
- **PATCH** version increments for bug fixes and internal optimizations.

Breaking changes must follow the deprecation process: at least 2 minor releases of deprecation warning before removal in the next MAJOR.

---

## Stable API Surface (v0.1.0)

The following items are **stable** and subject to SemVer compatibility guarantees:

### RangoClient Methods

- `RangoClient::open(storage, oplog, node_id) -> Result<Self, RangoError>`
- `RangoClient::collection(name) -> CollectionClient<'_, S>`
- `RangoClient::import_json(collection, path, progress) -> Result<ImportResult, RangoError>`
- `RangoClient::export_json(collection, path) -> Result<ExportResult, RangoError>`

### CollectionClient Methods

- `CollectionClient::insert_one(doc) -> Result<DocumentId, RangoError>`
- `CollectionClient::find_one(id) -> Result<Option<RangoDocument>, RangoError>`
- `CollectionClient::find_many() -> Result<Cursor, RangoError>`
- `CollectionClient::update_one(id, update) -> Result<bool, RangoError>`
- `CollectionClient::delete_one(id) -> Result<bool, RangoError>`

### Import/Export Types

- `ImportProgress` trait: `on_document(count)`, `on_error(line, error)`, `on_complete(imported, errors)`
- `NoOpProgress` struct (implements `ImportProgress`)
- `ConsoleProgress` struct (implements `ImportProgress`)
- `ImportResult` struct: `imported: usize, errors: usize`
- `ExportResult` struct: `exported: usize`

### Re-exported Types from `rango_types`

The following types from `rango-types` are re-exported and stable:

- `RankingExplainability`
- `RankingSignals`
- `RetrievalCandidate`
- `RetrievalCapabilityRequest`
- `RetrievalCapabilityResponse`
- `RetrievalSource`
- `RetrievalStatus`
- `RangoError` (if exposed in public SDK API)

### Re-exported Types from External Crates

- `rango_sdk::Document` — re-export of `bson::Document` for stable serialization
- `rango_sdk::Cursor` — newtype wrapper around `rango_core::Cursor`, providing stable iterator interface

---

## Unstable / Experimental Items

The following items are **experimental** and may change or be removed without notice, even within a minor version:

- `RangoClient::open_with_config(storage, oplog, node_id, config) -> Result<Self, RangoError>`
  - Configuration schema is not yet finalized; ADR pending.

- `DerivedReadLabel` struct and `DerivedReadLabel::derived_non_canonical()` method
  - Internal metadata for read classification; API may change.

- `SemanticProjectionRequest` struct and `SemanticProjectionRequest::new(...)` method
  - Semantic projection layer is under active development.

- `SemanticProjectionResponse` struct
  - Part of unstable semantic projection API.

- `TieredMemoryReadRequest` struct and its builder methods
  - Tiered memory management is still being refined.

- `TieredMemoryWriteRequest` struct and its builder methods
  - Tiered memory management is still being refined.

Users should avoid building critical logic on these APIs. Use only if you understand the contract may change.

---

## Breaking-Change Process

When a breaking change is necessary:

1. **Issue ADR** — File an ADR in `docs/adr/` explaining the motivation and migration path.
2. **Deprecation Phase** — Mark the old API with `#[deprecated(since = "x.y.z", note = "...")]` for **at least 2 minor versions**.
3. **Removal** — Delete the deprecated item in the next MAJOR release.

Example deprecation:

```rust
#[deprecated(since = "0.2.0", note = "Use `new_method()` instead. See ADR-XXX.")]
pub fn old_method() { /* ... */ }
```

SDKs must honor this timeline to give external integrators sufficient notice.

---

## Dependency Stability

To prevent version conflicts and hidden type leakage in downstream integrators:

- **Forbidden in public SDK signatures**: Direct types from `redb::*`, `axum::*`, `tokio::*` (runtime types like `tokio::task::JoinHandle`).
- **Permitted exceptions**:
  - `bson::Document` — allowed only via re-export as `rango_sdk::Document`.
  - `rango_core::Cursor` — allowed only via newtype wrapper `rango_sdk::Cursor`.
  - Standard library types (`Result`, `Option`, etc.).
  - `rango_types::*` — all types from the `rango-types` crate.

If you must introduce a new external type in the stable API, wrap it in a newtype or re-export it. Avoid exposing transitive dependencies directly.

---

## Testing Stability

To gate breaking changes and catch integration regressions:

- **Compile-Time Check**: `examples/sdk-stability/src/main.rs` must compile without modification across releases.
- **CI Integration**: The example is built in CI with `cargo build -p sdk-stability-example --locked`.
- **Coverage**: The example must exercise every item in the Stable API Surface above.

If your change causes `examples/sdk-stability/` to fail compilation, it is a breaking change and must follow the Breaking-Change Process.

---

## Known Limitation: Generic StorageEngine

The SDK is currently generic over `StorageEngine`:

```rust
pub struct RangoClient<S: StorageEngine> { /* ... */ }
```

This means users must link against a concrete storage adapter (e.g., `RedbStorage` from `rango-storage`) when building applications. Type erasure to `Box<dyn StorageEngine>` is intentionally deferred to a follow-up issue with its own ADR.

**Implication**: Users cannot freely swap storage backends at runtime in v0.1.0. This is a known constraint and will be revisited in a future release.

---

## Questions or Feedback?

If you have concerns about the stability policy or need to propose a breaking change:

1. Open an issue with the `type:design` label.
2. Reference this document and explain your use case.
3. Link to the corresponding ADR (or propose one).

Thank you for using Rango.
