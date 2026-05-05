# Memory Model — Rango Phase 2 (Memory Control Plane)

**Status:** Canonical specification for Phase 2 implementation  
**Last updated:** 2026-04-23  
**Scope:** Memory-first architecture with explicit state/history/artifact separation

---

## Overview

Rango's phase 8 memory-first architecture separates concerns across three envelope types:

1. **RecordEnvelope** — Materialized current state (read-write master copy)
2. **EventEnvelope** — Append-only immutable history (source of truth for replay and audit)
3. **ArtifactEnvelope** — Derived, rebuildable output (e.g., AI agent memory, summaries)

This separation is foundational for:
- **Deterministic replay:** replay events in order to reconstruct state identically
- **Idempotent sync:** deduplicate by `write_id` end-to-end
- **Conflict resolution:** maintain multi-version history for concurrent writes
- **Stateful AI workloads:** enable agents to replay their memory and history without ambiguity

---

## RecordEnvelope: Materialized State

### Purpose

RecordEnvelope represents the **current state** of a document after all mutations up to a given revision have been applied. It is NOT a source of truth for history; the oplog and EventEnvelope are the canonical history.

### Structure

```rust
pub struct RecordEnvelope {
    pub schema_version: u32,          // For evolution
    pub namespace: String,            // Ownership scope
    pub entity_id: String,            // Document id within namespace
    pub lineage_id: String,           // Derivation lineage across versions
    pub write_id: String,             // Dedup key
    pub source_node: String,          // Single-writer enforcer
    pub timestamp_ms: i64,            // Creation time (immutable)
    pub current_revision: String,     // HLC at which state applies
    pub sequence: u64,                // Ordering within namespace
    pub data: Document,               // User payload
    pub conflict_siblings: Vec<(String, String)>, // (revision, source_node)
}
```

### Invariants

- **schema_version ≥ 1:** Always versioned for evolution
- **namespace ≠ "":** Required for ownership and scoping
- **entity_id ≠ "":** Unique within namespace
- **lineage_id ≠ "":** Shared across all versions of this document; immutable once set
- **write_id ≠ "":** Matches the mutation that created/modified this record
- **source_node ≠ "":** Identifies the authoritative writer; enforces single-writer per namespace
- **timestamp_ms:** Immutable after first write; canonical for LWW ordering
- **current_revision:** Comparable HLC string; used for ordering and conflict detection
- **data:** May contain reserved fields (`_deleted`, `_conflicts`, etc.)
- **conflict_siblings:** Empty for non-conflicted documents; populated when concurrent writes create siblings

### Read-Write Semantics

- **Local reads:** Memory-resident record is read directly
- **Remote writes:** Sync pull applies EventEnvelopes to compute new RecordEnvelope via deterministic replay
- **Conflicts:** LWW by timestamp_ms; losing writes retained in conflict_siblings
- **Tombstone:** `_deleted: true` flag in data; document not removed until GC runs

### Lifecycle

```
RecordEnvelope birth:
  1. Local insert/update → Mutation → write_id generated
  2. Oplog append (EventEnvelope logged)
  3. Materialize RecordEnvelope from event
  4. Store in memory (or persistent storage)
  5. Sync push queued

RecordEnvelope update (conflict):
  1. Remote event pulled via sync
  2. LWW comparison: remote.timestamp_ms vs local.timestamp_ms
  3. If remote wins:
     - Remote's mutation applied
     - Local version moved to conflict_siblings
     - conflict_siblings truncated to max 10 entries
  4. If local wins:
     - Remote version added to conflict_siblings
     - RecordEnvelope unchanged
  5. New EventEnvelope appended to oplog (for conflict resolution event)
```

---

## EventEnvelope: Append-Only History

### Purpose

EventEnvelope represents a single mutation in the append-only operation log. Events are **never rewritten**, only appended. The event log is the canonical source of truth for:
- Replay and reconstruction
- Audit trails
- Crash recovery
- Conflict resolution chains

### Structure

```rust
pub struct EventEnvelope {
    pub schema_version: u32,           // For evolution
    pub namespace: String,             // Ownership scope
    pub entity_id: String,             // Document id
    pub lineage_id: String,            // Consistent across document lifetime
    pub write_id: String,              // Globally unique dedup key
    pub source_node: String,           // Authoritative writer
    pub timestamp_ms: i64,             // Creation time (canonical for LWW)
    pub sequence: u64,                 // Monotonic within namespace/source_node
    pub mutation_type: String,         // "insert" | "update" | "delete" | "conflict_resolution"
    pub mutation_data: Option<Document>, // Patch or full document
    pub is_tombstone: bool,            // Logical deletion marker
    pub hlc_revision: String,          // Causality tracking (HLC)
}
```

### Invariants

- **schema_version ≥ 1:** Always versioned for evolution
- **namespace ≠ "":** Required for ownership scoping
- **entity_id ≠ "":** Consistent within namespace
- **lineage_id ≠ "":** Immutable; shared across all events for a document
- **write_id ≠ "":** **Globally unique**, used for exactly-once deduplication
- **source_node ≠ "":** Must match namespace owner (single-writer enforcement)
- **timestamp_ms:** Immutable; canonical for LWW and ordering
- **sequence:** Monotonically increasing per (namespace, source_node) pair
- **mutation_type ≠ "":** Must be one of recognized types
- **hlc_revision ≠ "":** Causality and ordering information
- **is_tombstone:** True = logical deletion (entry not physically removed)
- **Append-only:** Never rewritten or deleted after append

### Special Cases

**Conflict Resolution Event:**
- `mutation_type = "conflict_resolution"`
- `mutation_data` contains metadata about resolved conflict
- Represents explicit resolution by application or policy engine
- Becomes part of canonical history

**Tombstone Event:**
- `mutation_type = "delete"`
- `is_tombstone = true`
- `mutation_data = None` or metadata only
- Document logically deleted; physically removed during GC

---

## ArtifactEnvelope: Derived Output

### Purpose

ArtifactEnvelope represents **derived state** computed from a RecordEnvelope and optionally informed by EventEnvelope history. Examples:

- Serialized AI agent memory buffer at checkpoint
- Computed summary or embedding
- Cached transformation or index
- Incremental backup snapshot

### Key Property

**Artifacts are NOT canonical.** They can be safely deleted and rebuilt from state + history without loss of durability.

### Structure

```rust
pub struct ArtifactEnvelope {
    pub schema_version: u32,
    pub namespace: String,
    pub source_record_entity_id: String,      // Which RecordEnvelope?
    pub source_record_lineage_id: String,     // Lineage tracking
    pub source_record_revision: String,       // At which revision?
    pub artifact_type: String,                 // "agent_memory", "summary", etc.
    pub content: Vec<u8>,                      // Opaque binary/JSON
    pub derived_at_timestamp_ms: i64,          // When computed?
    pub derived_by_node: String,               // Which node?
    pub parent_artifact_revision: Option<String>, // Incremental derivation?
}
```

### Invariants

- **schema_version ≥ 1:** Always versioned
- **namespace:** Inherited from source record
- **source_record_entity_id ≠ "":** Identifies the RecordEnvelope
- **source_record_lineage_id ≠ "":** Consistency check
- **source_record_revision ≠ "":** Immutable snapshot point
- **artifact_type ≠ "":** User-defined artifact class
- **content:** Opaque to core; interpretation depends on artifact_type
- **derived_at_timestamp_ms:** Immutable; marks derivation time
- **derived_by_node ≠ "":** Derivation authority
- **Rebuildable:** From source_record + event history

### Lifecycle

```
ArtifactEnvelope birth (stateful AI agent example):
  1. Agent processes EventEnvelope stream for its memory namespace
  2. Materializes conversational history into `content` buffer
  3. Creates ArtifactEnvelope with artifact_type="agent_conversation_buffer"
  4. Stores artifact (locally or in sync queue for remote persistence)

ArtifactEnvelope rebuild (after crash):
  1. Agent restarted; reads EventEnvelope history
  2. Replays events to reconstruct agent RecordEnvelope state
  3. Recomputes ArtifactEnvelope from current record + history
  4. Result: identical to pre-crash artifact (if deterministic derivation)

ArtifactEnvelope GC (cleanup):
  1. Stale artifact identified (e.g., source_record newer, parent_artifact stale)
  2. Artifact deleted (no data loss; can be recomputed)
  3. Freed space reusable for new artifacts
```

---

## State Machine: Event → Record → Artifact

### Transition Diagram

```
EventEnvelope (new) 
  ↓ (oplog append, dedup by write_id)
RecordEnvelope update or create
  ↓ (application logic, deterministic derivation)
ArtifactEnvelope (optional)
  ↓ (artifact lifecycle: cache, expire, rebuild)
Garbage collection
```

### Deterministic Application Order

For deterministic replay, apply EventEnvelopes in this strict order:

```
Sort key: (collection, timestamp, seq, doc_id, write_id)
  1. collection:       Replay scope in the runtime engine
  2. timestamp:        Canonical event ordering in replay batches
  3. seq:              Monotonic sequence tie-breaker
  4. doc_id:           Stable deterministic ordering per document
  5. write_id:         Final tie-breaker for deterministic idempotency
```

**Rationale:** This tuple matches `RangoEngine::apply_mutations_deterministic` and is the canonical replay source for Phase 1.

### Idempotency Guarantee

**Key Invariant:** Applying the same EventEnvelope twice (same write_id) produces identical RecordEnvelope state.

**Implementation:**
1. At server ingress: dedupe by write_id before oplog append (reject duplicate)
2. At oplog apply: dedupe by write_id before record update (skip already-applied)
3. At sync replay: dedupe by write_id before applying to local state
4. At local insert/update/delete: generate non-empty `write_id` before oplog append

**Test:** Replay event set N times → N RecordEnvelopeSnapshots must be byte-identical

---

## Backward Compatibility

### Evolution Rules

1. **Field addition:** New fields must have sensible defaults (e.g., `Option<T>`, empty string, zero, false)
2. **Field removal:** Deferred to next major schema version; document deprecation path
3. **Enum expansion:** New mutation_type or artifact_type values are additive; consumers must handle unknown types gracefully
4. **schema_version:** Bump version only when old consumers cannot safely ignore new fields

### Compatibility Levels

- **schema_version=1 → schema_version=2 upgrade:** Must define migration logic; can assume v1 old records
- **Downgrade (v2 → v1):** Not guaranteed; consumers must version-gate new features

---

## Conflict Handling

### LWW (Last-Write-Wins) Policy

- Compare by `timestamp_ms` (canonical event creation time)
- Higher timestamp wins; lower timestamp's RecordEnvelope moved to conflict_siblings
- `conflict_siblings` vector stores (revision, source_node) tuples (max 10 entries)
- Explicit `conflict_resolution` events can be emitted to log policy decisions

### Conflict Materialization

```rust
// Example: LWW resolution
if remote_event.timestamp_ms > local_record.timestamp_ms {
    // Remote wins
    let old_local = local_record.clone();
    apply_mutation_to_record(local_record, remote_event);
    local_record.conflict_siblings.push((
        old_local.current_revision.clone(),
        old_local.source_node.clone()
    ));
} else {
    // Local wins; track remote as sibling
    local_record.conflict_siblings.push((
        remote_event.hlc_revision.clone(),
        remote_event.source_node.clone()
    ));
}
```

---

## Compliance Rules for Crates

### crates/types

✅ **MUST:**
- Define RecordEnvelope, EventEnvelope, ArtifactEnvelope with all invariants
- Provide `validate()` methods that enforce invariants
- Provide serialization/deserialization (serde)
- Document reserved field names (e.g., `_deleted`, `_conflicts`)

### crates/core

✅ **MUST:**
- Enforce deterministic apply order (sort by tuple defined above)
- Implement idempotency checks (write_id dedup)
- Maintain RecordEnvelope materialization from EventEnvelope replay
- Prevent mutation of EventEnvelope history
- Maintain conflict_siblings correctly

❌ **MUST NOT:**
- Store AI domain semantics (e.g., "agent_memory", "conversation") in RecordEnvelope
- Use AI-specific terminology in apply paths
- Assume non-deterministic iteration for ordering

### crates/sync

✅ **MUST:**
- Preserve write_id end-to-end (client → server → oplog → apply)
- Implement single-writer validation per namespace
- Emit telemetry for non-owner rejection
- Support push/pull with EventEnvelope arrays

❌ **MUST NOT:**
- Leak artifact derivation logic into sync protocol
- Assume broadcast (multi-writer) semantics

### crates/server

✅ **MUST:**
- Reject non-owner writes at ingress (source_node mismatch)
- Dedupe by write_id before oplog append
- Emit counter for rejected writes

### Extension Layer (outside core)

✅ **MUST:**
- Consume EventEnvelope generic history
- Produce ArtifactEnvelope derived output
- Use artifact_type for self-identification

❌ **MUST NOT:**
- Modify EventEnvelope history
- Assume RecordEnvelope contracts beyond what's documented

---

## Summary

| Type | Role | Ownership | Mutability | History |
|------|------|-----------|-----------|---------|
| **RecordEnvelope** | Materialized state | Single-writer per namespace | Mutable (on remote apply) | Not canonical |
| **EventEnvelope** | Append-only history | Immutable | Append-only | Canonical |
| **ArtifactEnvelope** | Derived output | Derivation node | Rebuildable | Non-canonical |

The flow `event -> record materialization -> artifact derivation` is the foundation for deterministic replay, idempotent sync, and stateful AI workloads without semantic pollution of the core engine.

## Governance Metadata Compatibility

The canonical governance metadata set for envelopes and mutations is:
`id`, `namespace`, `tenant_id`, `type`, `rev`, `created_at`, `updated_at`, `source`,
`actor`, `lineage`, `schema_version`, `trust_score`, with optional `verified`, `expires_at`.

Compatibility requirements for these fields:

1. Producers MUST always emit required fields.
2. Consumers MUST reject records missing required governance metadata.
3. `verified` defaults to `None` when unknown and MUST NOT be coerced to `true`.
4. `expires_at` defaults to `None`; absence means no explicit expiry.
5. `trust_score` is bounded to `[0.0, 1.0]`; out-of-range values are invalid.
6. `updated_at >= created_at` is mandatory for deterministic lifecycle ordering.

## Control-Plane Alignment

This model is enforced through explicit control-plane APIs in `rango_core`:

- `write_path` validates metadata and evaluates trust before persistence.
- `read_path` applies retrieval gating and bounded-context filtering.
- `promotion_path` handles explicit tier promotion and sanitization.

Deterministic durability semantics:

- Snapshot restore + replay must converge with full replay.
- Rollback targets explicit snapshot units.
- Sync dedup is tenant-scoped and idempotent by `write_id`.
