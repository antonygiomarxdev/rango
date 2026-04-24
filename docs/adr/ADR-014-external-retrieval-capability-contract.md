# ADR-014: External Retrieval Capability Contract

- Status: Accepted
- Date: 2026-04-24
- Deciders: Rango maintainers
- Supersedes: None
- Related: ADR-009, ADR-010, ADR-013

## Context

Phase 05 adds advanced retrieval while preserving canonical truth boundaries. The roadmap and PRD require vector and graph retrieval to stay external capabilities, never authoritative storage in core truth paths.

## Decision

### D05-01: Deterministic ranking governance

The ranking formula is locked as `v1`:

`score = 0.35*relevance + 0.30*trust + 0.20*recency + 0.15*provenance`

Every ranked candidate includes explainability metadata with weighted components and total score.

### D05-02: External adapter boundary and primary graph backend

Vector and graph retrieval are implemented through adapter interfaces in `crates/server/src/retrieval/adapters.rs`.
Reference boundaries are:

- Vector boundary: Qdrant-compatible adapter shape
- Graph boundary: Neo4j/`neo4rs`-compatible adapter shape

Adapters are optional external capabilities. Canonical core/runtime truth remains oplog + materialized state + checkpoints.

### D05-03: Degradation semantics

If external retrieval adapters fail or are unavailable, retrieval reads return `200` with:

- `retrieval_status = degraded`
- `canonical_fallback = true`
- Empty advisory candidates

Canonical operations continue unaffected. Canonical read/write/promotion failure behavior is unchanged.

## Consequences

- Retrieval remains extension-only and does not introduce vector/graph engine logic into core truth internals.
- Ranking behavior is deterministic, testable, and explainable.
- Outages in external services degrade safely without blocking canonical control-plane operations.
- Tenant/namespace predicates and bounded context filtering remain mandatory gates for retrieval outputs.
