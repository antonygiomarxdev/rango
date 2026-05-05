# ADR-010: Memory Control Plane Governance Boundaries

- Status: Accepted
- Date: 2026-04-23
- Deciders: Rango maintainers
- Supersedes: none

## Context

Phase 2 (Memory Control Plane) introduces governance metadata to support tenant isolation,
provenance, trust, replay determinism, and policy-gated tier promotion. Without explicit boundaries,
the core substrate risks absorbing semantic/retrieval product semantics that belong to later phases.

## Decision

1. v1 canonical truth remains append-only operations plus materialized state.
2. Core APIs expose explicit `write_path`, `read_path`, and `promotion_path` contracts.
3. Envelopes and mutations require governance metadata:
   `id`, `namespace`, `tenant_id`, `type`, `rev`, `created_at`, `updated_at`,
   `source`, `actor`, `lineage`, `schema_version`, `trust_score`, optional `verified`, `expires_at`.
4. Sync/server enforce tenant+namespace isolation and emit auditable policy decisions.
5. Snapshot/rollback/restore and idempotent apply are deterministic requirements, not optional behavior.

## Scope Boundaries

### In v0.1.0 Core

- Tenant-aware contracts and validation.
- Policy hooks with deterministic invocation order.
- Poisoning baseline controls (trust-aware gating, sanitization hooks, auditable outcomes).
- Deterministic replay/snapshot/rollback/idempotent sync behavior.

### Deferred to v0.2.0

- Semantic consolidation logic and higher-order derived memory policies.
- Rich ranking heuristics for semantic retrieval.

### Deferred to post-v0.3.0

- Vector/graph-native retrieval primitives in core.
- Workflow/orchestration semantics in core contracts.

## Consequences

- Core remains substrate-focused and reusable across domains.
- Semantic/retrieval capabilities can evolve without destabilizing durability/sync guarantees.
- Security posture improves via testable tenant and poisoning controls.
