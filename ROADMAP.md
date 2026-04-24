# Rango Roadmap

This roadmap is the public execution map for Rango.  
Strategy lives here. Actionable implementation work lives in GitHub Issues.

## Direction

Rango is a durable memory and state substrate for stateful AI systems:

- memory-first, document-native, local-first
- append-only history + materialized current state
- replay, sync, governance, and safety as first-class concerns
- strict separation between canonical truth and derived projections

## Phases

### Phase 1: Durable Substrate

Goal: close the canonical core for durable state/history and deterministic replay.

Success criteria:

- append-only oplog + materialized state + checkpoints operational
- deterministic apply/replay verified by tests
- sync idempotency via `write_id`
- canonical envelopes and metadata in runtime paths

### Phase 2: Memory Control Plane Basics

Goal: enforce explicit memory control paths.

Success criteria:

- typed `write_path`, `read_path`, and `promotion_path`
- deterministic hook execution order
- policy hooks for classification, trust, and validation
- bounded read assembly for model context

### Phase 3: Security and Governance Enforcement

Goal: move memory safety from guidelines to runtime enforcement.

Success criteria:

- tenant/namespace isolation enforced end-to-end
- provenance and trust fields enforced in write/read paths
- poisoning containment controls (sanitization + anomaly handling)
- auditability of memory operations

### Phase 4: Semantic Projections

Goal: support optional derived semantic memory without polluting canonical truth.

Success criteria:

- projections for facts/summaries/preferences with lineage
- governed promotion from episodic to semantic memory
- rebuild/invalidation behavior validated

### Phase 5: Advanced Retrieval Capabilities

Goal: provide external retrieval capabilities as pluggable projections.

Success criteria:

- vector and graph capability boundaries outside core
- trust-aware ranking + bounded context assembly
- stable capability contracts and fallback behavior

## GitHub Execution Model

- One issue per executable unit.
- Every issue must map to exactly one roadmap phase label (`phase:1` ... `phase:5`).
- Every issue should target a milestone (`v0.1.0`, `v0.2.0`, ...).
- Pull requests should close issues explicitly (`Closes #123`).

## Update Policy

- Update this file only for phase goals, phase criteria, and sequencing.
- Do not track task-level checklists here; keep those in GitHub Issues/Projects.
