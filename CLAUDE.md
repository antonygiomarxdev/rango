# Rango — Project Instructions

This project uses the GSD (Get Shit Done) workflow. All planning documents live in `.planning/` (local-only, not versioned).

## Current State

- **Project:** Rango (Project Atlas)
- **Phase:** 1 — Direction & Foundation
- **Goal:** Build a local-first embedded document database in Rust
- **Mode:** YOLO (auto-approve, execute directly)
- **Granularity:** Fine

## Key Context

- Local-first, offline-write, cloud-sync embedded document DB
- BSON-native, MongoDB CRUD-compatible API surface
- Workspace of crates: `types`, `core`, `storage`, `index`, `query`, `oplog`, `sync`, `server`, `sdk-rust`, `cli`
- Target: IoT gateways, edge devices, field operations
- Quality over speed — no hard deadlines, phases complete when success criteria are met
- Core value: sub-millisecond local reads, offline writes without data loss, reliable incremental sync

## Workflow Rules

1. **Always use GSD skills** when applicable (`/gsd-discuss-phase`, `/gsd-plan-phase`, `/gsd-execute-phase`, etc.)
2. **Check `.planning/PROJECT.md`** for current context before making architectural decisions
3. **Check `.planning/ROADMAP.md`** for phase goals and success criteria
4. **ADRs** must be written for all significant architectural decisions
5. **Tests are non-negotiable** — unit, integration, recovery, property tests, benchmarks
6. **No premature optimization**, but design for extensibility (traits for storage, index, sync transport)

## Next Step

Run `/gsd-discuss-phase 1` to gather context and clarify approach for Direction & Foundation.

## References

- `.planning/PROJECT.md` — Project vision, constraints, key decisions
- `.planning/REQUIREMENTS.md` — v1 requirements with REQ-IDs
- `.planning/ROADMAP.md` — Phase structure and success criteria
- `.planning/STATE.md` — Current execution state
- `.planning/research/` — Domain research (STACK, FEATURES, ARCHITECTURE, PITFALLS, SUMMARY)
