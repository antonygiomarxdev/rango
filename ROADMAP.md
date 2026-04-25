# Rango Roadmap

This roadmap is the public execution map for Rango.  
Strategy lives here. Actionable implementation work lives in GitHub Issues.

## Direction

Rango is a durable memory and state substrate for stateful AI systems:

- memory-first, document-native, local-first
- append-only history + materialized current state
- replay, sync, governance, and safety as first-class concerns
- strict separation between canonical truth and derived projections

## Product Identity

Rango is not a generic database product and not a workflow engine.

Rango is:

- a memory/state substrate for stateful AI runtimes
- canonical state + append-only episodes + governed derivations
- local-first embedded runtime with sync and replay

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

## Execution Focus by Version

### v0.1.0 (Adoption Baseline)

Deliver a usable and trustworthy integration baseline for external products.

Workstream A — substrate closure:
- canonical metadata enforcement end-to-end in write/read paths
- policy enforcement in control-plane hooks (write/read/promotion)
- snapshot/rollback deterministic recovery tests
- tenant/namespace isolation hardening in sync/server paths

Workstream B — external contract:
- integration-ready SDK contract and onboarding path (OpenClaw + generic)
- OpenClaw smoke integration running in CI on every PR
- migration/upgrade guide v0.0 -> v0.1 with `rango doctor` upgrade checks

### v0.2.0 (Hardening Baseline)

Make security/governance observable, production-safe, and adoptable from polyglot stacks.

Workstream C — audit and observability:
- audit trail completeness for memory operations
- anomaly signals and containment hooks
- OpenTelemetry metrics and tracing contract (hub + embedded)
- health/readiness endpoints and structured logs in `rango-server`

Workstream D — public trust moat:
- adversarial benchmark suite (poisoning, cross-tenant leak, byte-exact replay)
- public benchmark results + "Memory != Storage" thesis post
- compliance posture document (audit, retention, isolation)

Workstream E — polyglot DX:
- Python binding via PyO3 (minimum SDK surface, wheels on 3 OSes)
- TS/Node binding via napi-rs (parity with Python binding)
- reference integration: LangGraph agent on Rango
- reference integration: CrewAI agent on Rango

### v0.3.0 (Capabilities Baseline)

Enable advanced retrieval capabilities without contaminating core truth and harden multi-node operation.

Workstream F — capability contracts:
- external vector/graph capability contracts
- adapter conformance kit (mock + golden fixtures)
- reference adapters: pgvector, Qdrant
- trust-aware ranking input contract + bounded context assembly docs

Workstream G — operability:
- backup/restore CLI with optional S3 sink
- multi-node sync hardening (partition/heal regression suite)
- Grafana reference dashboard wired to OTel metrics

## GitHub Execution Model

- One issue per executable unit.
- Every issue must map to exactly one roadmap phase label (`phase:1` ... `phase:5`).
- Every issue should target a milestone (`v0.1.0`, `v0.2.0`, ...).
- Pull requests should close issues explicitly (`Closes #123`).

## Update Policy

- Update this file only for phase goals, phase criteria, and sequencing.
- Do not track task-level checklists here; keep those in GitHub Issues/Projects.
