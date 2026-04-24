# Rango - Project Instructions

This project uses the GSD workflow. Planning files are local in `.planning/`.

## Current Direction

- Project: Rango
- Goal: build a memory-first, document-native, local-first substrate for stateful AI systems
- Layer discipline: substrate only (no workflow product semantics in core)

## Core Context

- Canonical core: append-only oplog + materialized current state + deterministic replay
- v1 priority: durability, isolation, provenance, sync foundations
- Storage backend: `redb` by default, behind swappable `StorageEngine`
- Crates: `types`, `core`, `storage`, `index`, `query`, `oplog`, `sync`, `server`, `sdk-rust`, `cli`

## Workflow Rules

1. Use GSD skills when applicable.
2. Check `.planning/PROJECT.md` and `.planning/ROADMAP.md` before major architectural decisions.
3. Write ADRs for significant architecture changes.
4. Keep tests non-negotiable: unit/integration/recovery/property/benchmark as relevant.
5. Avoid premature optimization while preserving extension points.

## Next Step

Run `/gsd-discuss-phase 1` when starting a new planning reset.
