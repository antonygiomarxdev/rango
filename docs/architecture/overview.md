# Rango Architecture Overview

## Positioning

Rango is a local-first memory substrate with deterministic durability and sync semantics.
Core contracts remain domain-neutral: no workflow orchestration semantics in `crates/core` or `crates/types`.

## Crate Boundaries

- `rango-types`: canonical contracts (envelopes, mutation metadata, checkpoints, snapshot/rollback units).
- `rango-core`: engine + control plane (`write_path`, `read_path`, `promotion_path`) and deterministic replay.
- `rango-oplog`: append-only operation log and durable sequencing.
- `rango-sync`: push/pull protocol, tenant-aware transport, checkpoint progression.
- `rango-server`: authenticated ingress/egress, isolation enforcement, idempotent remote apply.

## Memory Control Plane Responsibilities

The control plane in `rango_core::control_plane` owns policy decisions:

- Write validation and trust scoring before persistence.
- Retrieval gating and bounded-context filtering before serving reads.
- Explicit promotion gating/sanitization for tier transitions (including episodic -> semantic).
- Auditable allow/reject/sanitize decisions with reason codes.

## Deterministic Guarantees

- Canonical truth: append-only operations + materialized state.
- Snapshot + replay restore converges to the same state as full replay.
- Remote sync transport is at-least-once; apply path is idempotent by tenant-scoped `write_id`.
- Duplicate or out-of-order mutation batches remain deterministic.

## Recovery and Replay Contract

Snapshot-anchored recovery ensures deterministic restoration after crash or explicit rollback:

- **Snapshot as canonical anchor:** A snapshot captures the complete materialized state at a sequence boundary (`base_seq`), allowing deterministic recovery without replaying from genesis.
- **Bounded replay:** After restoring from a snapshot, only mutations _after_ `base_seq` are replayed, capping recovery I/O to O(seqs above snapshot).
- **Deterministic convergence:** Snapshot + bounded replay of post-snapshot mutations yields identical state as full replay from genesis, satisfying the Deterministic Guarantees contract.
- **Crash recovery workflow:** On restart, restore from the latest available snapshot, then replay mutations from the oplog in sequence, using `tenant_id` and `write_id` to enforce idempotency.
- **Explicit rollback:** The `rollback_to_snapshot` API validates the rollback target and enforces that the target sequence cannot precede the snapshot's base sequence, preventing accidental state loss.

**Regression suite:** See `crates/core/tests/recovery_tests.rs` for end-to-end tests covering crash simulation with persistent storage, bounded replay correctness, and property-based determinism validation.

## Scope Boundaries

- **v1 core:** state/episodic/artifact durability + policy-governed semantic promotion hooks.
- **v0.2.0-v0.3.0:** semantic consolidation, operability, and sync hardening.
- **post-v0.3.0:** advanced retrieval (vector/graph) as external projections, not core substrate.
