# Rango Architecture Overview (Phase 09)

## Positioning

Rango v1 is a local-first memory substrate with deterministic durability and sync semantics.
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

## Scope Boundaries

- **v1 core:** state/episodic/artifact durability + policy-governed semantic promotion hooks.
- **v1.5-v2:** semantic consolidation and higher-order derived memory strategies.
- **v2+:** advanced retrieval (vector/graph) outside core substrate.
