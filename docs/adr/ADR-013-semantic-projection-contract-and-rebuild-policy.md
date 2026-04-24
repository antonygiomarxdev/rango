# ADR-013: Semantic Projection Contract and Rebuild Policy

- Status: Accepted
- Date: 2026-04-24
- Deciders: Rango maintainers
- Supersedes: None
- Related: ADR-010, ADR-011, ADR-012

## Context

Phase 04 introduces semantic projections as a derived tier. The project requires strict separation between canonical truth and semantic outputs, with auditable promotion and deterministic rebuild rules.

## Decision

### D04-01: Projection persistence contract

Semantic projection writes are persisted as derived artifacts only, with mandatory lineage, source revision, trust metadata, and tenant/namespace scope.

### D04-02: Invalidation and rebuild trigger policy

Invalidation and rebuild use source-revision watermark and explicit rebuild triggers. Rebuild uses canonical inputs and emits derived outputs only.

### D04-03: Read semantics

Semantic reads are opt-in and explicitly marked `derived=true` and `canonical=false`. Canonical reads remain default.

### D04-04: Scope lock

No vector indexing, embeddings, or graph traversal behavior is introduced in core/type/server/sdk runtime for Phase 04.

## Consequences

- Promotion gates remain enforceable and auditable through existing control-plane boundaries.
- Derived artifacts can be safely invalidated/rebuilt without canonical mutation.
- SDK consumers receive stable typed semantic/tier APIs with explicit derived labeling.
- Advanced retrieval remains deferred to Phase 05+.
