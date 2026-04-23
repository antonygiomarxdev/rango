# Memory Layers (Control-Plane Governed)

## Layer 0: Canonical Durability (v1)

- Append-only oplog + materialized state.
- Deterministic replay ordering and snapshot restore.
- Canonical metadata: tenant, provenance, trust, lifecycle fields.

## Layer 1: Episodic History (v1)

- Immutable event stream for replay/audit.
- Trust/policy evaluation occurs at write and retrieval gates.

## Layer 2: State Memory (v1)

- Mutable current state documents.
- Reads/writes always routed through explicit control-plane boundaries.

## Layer 3: Semantic Memory (v1.5-v2)

- Derived non-canonical memory promoted explicitly from lower tiers.
- Promotion must pass sanitization + policy gate.
- No implicit semantic writes in CRUD paths.

## Layer 4: Retrieval Extensions (v2+)

- Vector/graph retrieval systems consume exported events/artifacts.
- Implemented outside core substrate; replaceable and optional.

## Governance Rules

- Tenant and namespace isolation is mandatory on sync ingress/egress.
- Provenance/trust metadata is mandatory on envelope/mutation contracts.
- Policy outcomes are auditable (`allow`, `sanitize`, `reject` + reason).
- Workflow-product semantics are out of scope for core contracts.
