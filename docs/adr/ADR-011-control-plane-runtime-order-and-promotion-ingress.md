# ADR-011: Control Plane Runtime Order and Promotion Ingress

- Status: Accepted
- Date: 2026-04-23
- Deciders: Rango maintainers
- Supersedes: ADR-010 (clarifies runtime enforcement details)

## Context

Phase 02 found contract-vs-runtime drift in control-plane behavior:

- `promotion_path` existed in `rango_core`, but there was no canonical server ingress calling it.
- Read-path order in docs drifted from executable runtime order.
- Pull filtering correlated filtered candidates to mutations using payload debug strings, which is ambiguous when patch payloads are duplicates.

## Decision

1. Promotion is a first-class runtime path exposed by server ingress at `POST /promote`.
2. Runtime promotion flow must invoke `control_plane.promotion_path` before append.
3. Canonical read stage order is fixed as:
   `read.gate -> read.audit -> read.anomaly -> read.filter`.
4. Pull bounded filtering is mapped by stable mutation identity (`seq` + `write_id`), never payload debug text.
5. Substrate boundary lock remains in force: no workflow/product semantics in `core`, `types`, or `server`.

## Consequences

- `write_path`, `read_path`, and `promotion_path` are all runtime-enforced at typed server boundaries.
- Hook ordering is deterministic and now aligned across runtime, tests, and docs.
- Duplicate patch payloads cannot cause ambiguous read filtering behavior.
- Governance/audit outcomes are consistent across `/push`, `/pull`, and `/promote`.
