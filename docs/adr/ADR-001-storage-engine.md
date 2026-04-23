# ADR-001: Storage Engine Decision

## Status
Pending — will be decided after Phase 2 spike benchmark.

## Context
Necesitamos un motor de persistencia embebido en Rust para Rango.
Las opciones principales son:

- **fjall** (LSM-tree): Write-optimized, múltiples keyspaces nativos, compilación rápida.
- **redb** (B-tree): MVCC nativo, ACID, formato estable.

## Decision
TBD — spike benchmark evaluará write throughput, read latency y crash recovery.

## Consequences
- El trait `StorageEngine` se diseñó para ser agnóstico al backend.
- Implementaciones concretas irán en crates separados (`storage-fjall`, `storage-redb`).
