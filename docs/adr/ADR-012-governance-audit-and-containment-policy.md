# ADR-012: Governance Audit Durability, Containment Policy, and Rollback Contract

- Status: Accepted
- Date: 2026-04-23
- Deciders: Rango maintainers
- Supersedes: None

## Context

Phase 03 requires runtime security/governance behavior to be substrate-enforced and deterministic:

- GOV-02..GOV-05 require tenant-safe enforcement, auditable policy outcomes, and adversarial containment behavior.
- DUR-01..DUR-03 require explicit rollback semantics and deterministic replay convergence.
- Prior behavior emitted policy decisions in API responses but did not persist them as canonical substrate evidence.
- Anomaly signaling existed as hooks but did not drive deterministic containment transitions in runtime paths.

## Decision

1. D03-01 (Audit persistence): Governance outcomes are persisted as canonical substrate evidence in oplog records with `metadata.type = "governance_audit"` and linkage fields (`tenant_id`, `namespace`, `write_id`, `stage`, `decision`, `reason`).
2. D03-02 (Containment policy): Containment is deterministic per `(tenant_id, namespace)` and transitions as:
   `normal -> throttle -> reject` on reject bursts, with explicit cooldown reset.
3. D03-03 (Rollback semantics): Rollback is a first-class operation (`rollback_to_snapshot`) that validates snapshot identity/bounds, replays a bounded window, and emits explicit rollback audit output.
4. D03-04 (Scope lock): All enforcement remains substrate-only in core/server/sync layers; no workflow/product semantics are introduced.

## Consequences

- Policy outcomes are reconstructable from substrate history, not only from transient response payloads.
- Runtime containment behavior is reproducible and test-controlled for anomaly burst scenarios.
- Replay/rollback operations have deterministic, auditable semantics with explicit contracts.
- Sync and server runtime paths remain tenant-safe and bounded to substrate enforcement concerns.
