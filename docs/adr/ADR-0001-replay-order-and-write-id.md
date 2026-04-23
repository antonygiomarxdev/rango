# ADR-0001: Replay Tuple Canonicalization and Local Write ID Policy

## Status
Accepted

## Context

Phase 01 closes substrate-level claim gaps around deterministic replay and idempotent writes.
Two open questions required an explicit architectural decision:

1. What replay tuple is canonical when docs and code drift?
2. Whether empty local `write_id` values are acceptable in the engine.

## Decision

1. Canonical replay tuple is locked to runtime engine behavior:
   `(collection, timestamp, seq, doc_id, write_id)`.
2. Local insert, update, and delete paths must generate a non-empty `write_id` before oplog append.
3. `promotion_path` runtime integration is deferred to Phase 02; Phase 01 does not claim it as active runtime wiring.

## Rationale

- Deterministic replay must have exactly one canonical sort tuple to avoid drift.
- End-to-end idempotency by `write_id` requires local writes to participate in the same invariant.
- Deferring promotion wiring keeps Phase 01 strictly focused on substrate correctness gaps.

## Consequences

### Positive

- Documentation and runtime order are aligned and testable.
- Local and remote mutation paths share the same idempotency contract.
- Phase 01 claims now match executable behavior.

### Tradeoff

- Existing docs that referenced namespace/source-node ordering needed updates.
- Promotion runtime wiring remains future work until Phase 02.

## Evidence

- Runtime sort implementation: `crates/core/src/engine.rs` (`apply_mutations_deterministic`)
- Local write-id generation: `crates/core/src/engine.rs` (`insert_one`, `update_one`, `delete_one`)
- Invariant test: `crates/core/tests/local_write_id_non_empty.rs`
- Doc alignment: `docs/architecture/memory/model.md`

## Follow-up

- Phase 02 must add runtime `promotion_path` integration coverage and associated tests.
