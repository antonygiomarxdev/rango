# Changelog

All notable changes to Rango will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial public release of the Rango embedded document database
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

[Unreleased]: https://github.com/antonygiomarxdev/rango/compare/HEAD...HEAD
