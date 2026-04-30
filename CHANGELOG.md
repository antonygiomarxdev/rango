# Changelog

All notable changes to Rango will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] — 2026-04-30

### Added

#### Security & Governance
- **Tenant isolation fixes** (#10): `process_pull` now returns audit records for cross-tenant/node mismatches. Added `concurrent_isolation.rs` and `audit_completeness.rs` integration tests.
- **Complete audit trail** (#12): Governance decisions are persisted as durable audit evidence (`__governance_audit`). Push/promote rejection paths now emit audit records. Audit records no longer advance user-data checkpoints.
- **Anomaly detection** (#13): Anomaly signal hooks evaluate every governance stage. Containment gate blocks push/pull/promote during reject mode with cooldown.
- **Graceful degradation** (#19): `DegradingStorage` wrapper monitors disk space and degrades to read-only when available space drops below threshold (default 100MB). Auto-recovers when space frees up.

#### Observability
- **OpenTelemetry metrics** (#22): `RangoMetrics` with counters for push/pull/promote/retrieval/rejection. Tenant and namespace labels. Health check and metrics wiring in server handlers.
- **Health endpoints** (#25): `/health` (liveness) and `/ready` (readiness, checks oplog accessibility). Structured JSON logs via `tracing-subscriber` with `RANGO_LOG_FORMAT=json`.

#### SDK & Bindings
- **Python binding** (#35): `crates/python/` with PyO3 + maturin. `RangoClient` with CRUD operations, auto-`$set` wrapping for updates, flexible ID parsing (UUID/ObjectId/string). `Mapping[str, object]` type stubs (no `Any`).
- **TypeScript/Node.js binding** (#37): `crates/node/` with napi-rs. JSON string interface with TypeScript wrapper (`rango.ts`). Collection class with typed methods.

#### Retrieval Adapters
- **Adapter contracts** (#14): `VectorRetrievalAdapter` and `GraphRetrievalAdapter` traits with `health_check()` and `adapter_name()`. Reference implementations for Qdrant (vector) and Neo4j (graph). 9 conformance tests verifying tenant scoping, parameterized queries, and ranking signals.

#### Benchmarks & Tooling
- **Adversarial benchmarks** (#28): Criterion suite with deterministic seeds. Tests: poisoning rejection latency, cross-tenant leak check, replay determinism, push throughput, pull latency, audit persistence.
- **CLI audit report** (#24): `rango audit` subcommand reads `__governance_audit` entries. Supports text/json/csv output with tenant/namespace filtering.

### Changed
- Workspace root converted to package+workspace for benchmark hosting (#28)
- `.gitignore` updated to exclude `*.proptest-regressions` and Python build artifacts

### Fixed
- Audit records (`__governance_audit`) no longer counted in user-data checkpoints (#12)
- `process_pull` cross-tenant mismatch returns `PullResponse` with audit instead of raw 403 (#10)

## [0.1.0] — 2026-04-23

### Added
- Initial public release of the Rango memory substrate
- `rango-types`: shared primitives (DocumentId, Revision, Mutation, Checkpoint)
- `rango-storage`: pluggable KV storage engine with AES-256-GCM encryption
- `rango-index`: primary and secondary B-tree index management
- `rango-query`: filter, projection, sort, limit, skip + `$set`/`$unset`/`$inc`
- `rango-oplog`: persistent append-only operation log with compaction
- `rango-sync`: HTTP/JSON push-pull sync with Last-Write-Wins conflict resolution
- `rango-core`: main engine orchestrating all subsystems
- `rango-server`: Axum-based sync server (push/pull endpoints)
- `rango-sdk`: public Rust SDK with import/export and migration utilities
- `rango`: CLI tool (init, inspect, import, export, bench, doctor, sync)
- Criterion benchmarks and 53+ unit/integration/property tests
- Architecture docs, ADRs, and sync protocol specification

### Changed
- Repository licensing moved to Business Source License 1.1 with a project-specific Additional Use Grant
- Crate manifests now point to the repository license file and are marked `publish = false` until package distribution is explicitly enabled
- `rango doctor` now exits with non-zero status on incompatible workspaces (#27)

### Fixed
- Add v0.0 → v0.1 migration guide and upgrade checks (`docs/operations/migration-v0.0-to-v0.1.md`)

[Unreleased]: https://github.com/antonygiomarxdev/rango/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/antonygiomarxdev/rango/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/antonygiomarxdev/rango/releases/tag/v0.1.0
