# ADR-008: State/History Separation and Deterministic Replay

## Status
Accepted

## Context

Phase 8 introduces explicit support for stateful AI workloads that require:
- **Deterministic replay:** Exact reconstruction of state after crash/reconnect
- **Audit trail integrity:** Immutable history of all mutations
- **Conflict resolution:** Safe merging of concurrent writes from multiple nodes
- **Idempotent sync:** Exactly-once semantics across network failures

Prior phases mixed mutable state snapshots (RecordEnvelopes) with append-only history (EventEnvelopes) without clear separation. This made it difficult to reason about replay ordering, idempotency guarantees, and conflict resolution.

**Problem:** Without explicit separation and strict replay ordering, the same mutation set could produce different RecordEnvelopes depending on:
- Hash map iteration order in apply paths
- Timestamp tie-breaking in LWW
- Non-deterministic conflict resolution

This violates determinism requirements for stateful AI agents that depend on reproducible memory materialization.

## Decision

**Enforce strict separation between:**
1. **RecordEnvelope** — Materialized current state (mutable projection)
2. **EventEnvelope** — Append-only immutable history (canonical source of truth)
3. **Deterministic apply order** — Sort by (namespace, timestamp_ms, sequence, source_node, write_id)

**Guarantees:**
- `RecordEnvelope` is a materialization of EventEnvelope history up to a given revision
- `EventEnvelope` is never modified after append; all mutations become new events
- Replay of the same EventEnvelope set in sorted order produces byte-identical RecordEnvelope and conflict metadata
- Idempotency is enforced at three levels: server ingress (dedupe), oplog (skip), and apply (skip)

## Rationale

### Why Separate?

**RecordEnvelope** (mutable state):
- Represents current view suitable for queries and application logic
- Reduces compute cost vs. replaying entire history on every read
- Can be optimized for memory/disk layout

**EventEnvelope** (immutable history):
- Audit trail immune to tampering
- Foundation for replay, recovery, and conflict resolution
- Enables replication and sync across nodes
- Supports incremental sync via checkpoints

### Why Deterministic Replay?

Stateful AI agents (e.g., LLM assistants) maintain conversation history and memory. If replayed history produces different RecordEnvelope state, the agent's memory becomes corrupted or inconsistent. This breaks trust for critical applications (field operations, medical IoT, etc.).

Example: Agent with conversation buffer:
```
Event 1: User says "temperature is 23°C" (timestamp=1000)
Event 2: System processes sensor (timestamp=2000)
```

If Event 2 and Event 1 replay in different order, the buffer contents differ → agent memory corrupted.

**Solution:** Strict sort order ensures Event 1 always applies before Event 2, regardless of network arrival order or node restart sequence.

### Why Specific Sort Key?

```
Sort: (namespace, timestamp_ms, sequence, source_node_hash, write_id)
```

- **namespace:** Ownership scope; allows parallel processing per namespace
- **timestamp_ms:** LWW (Last-Write-Wins) canonical; breaks semantic ties
- **sequence:** Monotonic per source; ensures causal ordering
- **source_node_hash:** Stable deterministic node ordering; breaks hash map iteration ambiguity
- **write_id:** Global dedup key; final tie-breaker (globally unique)

### Why Idempotency at Three Levels?

1. **Server ingress:** Reject duplicate write_id before oplog append (fast failure)
2. **Oplog apply:** Skip events already applied (idempotent from crash during apply)
3. **Sync replay:** Skip events already in local state (idempotent for offline->online transitions)

This defense-in-depth prevents data loss and ensures exactly-once semantics despite:
- Network retries (client sends same write_id twice)
- Server crashes (partial oplog write)
- Restart with incomplete apply (some events applied, others not)

## Consequences

### Positive

- **Deterministic replay:** Identical RecordEnvelope produced from same EventEnvelope set
- **Crash safety:** State fully recoverable from oplog + event history
- **Conflict transparency:** All conflict resolution decisions logged as events
- **Audit trail:** Complete immutable history for compliance/debugging
- **Idempotent sync:** Network retries don't cause duplicates or state divergence
- **Foundation for AI workloads:** Agents can replay history and materialize consistent memory

### Negative

- **Replay cost:** Crash recovery requires full or partial replay (O(N) where N = events since last checkpoint)
- **Deduplication overhead:** Three levels of dedup checks (minor, but non-zero)
- **State explosion:** Conflict siblings retained (max 10 per RecordEnvelope); could consume memory if conflicts frequent
- **Schema versioning complexity:** Future schema evolution must preserve sort order determinism

## Implementation

### RecordEnvelope Structure (types crate)

```rust
pub struct RecordEnvelope {
    pub schema_version: u32,
    pub namespace: String,
    pub entity_id: String,
    pub lineage_id: String,
    pub write_id: String,
    pub source_node: String,
    pub timestamp_ms: i64,
    pub current_revision: String,
    pub sequence: u64,
    pub data: Document,
    pub conflict_siblings: Vec<(String, String)>, // (revision, source_node)
}
```

### EventEnvelope Structure (types crate)

```rust
pub struct EventEnvelope {
    pub schema_version: u32,
    pub namespace: String,
    pub entity_id: String,
    pub lineage_id: String,
    pub write_id: String,
    pub source_node: String,
    pub timestamp_ms: i64,
    pub sequence: u64,
    pub mutation_type: String,
    pub mutation_data: Option<Document>,
    pub is_tombstone: bool,
    pub hlc_revision: String,
}
```

### Apply Order (core crate)

```rust
// Sort EventEnvelopes before apply
events.sort_by(|a, b| {
    a.namespace.cmp(&b.namespace)
        .then(a.timestamp_ms.cmp(&b.timestamp_ms))
        .then(a.sequence.cmp(&b.sequence))
        .then(hash_node(&a.source_node).cmp(&hash_node(&b.source_node)))
        .then(a.write_id.cmp(&b.write_id))
});

for event in events {
    if !dedupe.contains(&event.write_id) {
        apply_event_to_record(&mut record, event);
        dedupe.insert(event.write_id);
    }
}
```

### Tests

- **Replay determinism test:** Apply same event set 10x, assert identical RecordEnvelope each time
- **Dedup test:** Apply event twice, assert second is skipped
- **Conflict sibling test:** Concurrent writes from two nodes produce conflict_siblings
- **Idempotency matrix test:** Offline writes + reconnect + replay produce correct state

## Non-Goals

- **Multi-writer per namespace:** Single-writer enforced by separate ADR-010
- **Auto-conflict resolution:** Conflicts logged; resolution is policy/application-driven
- **External merge sort:** Replay assumes events fit in memory; spill-to-disk deferred to future phase
- **Artifact versioning:** Artifacts rebuild on demand; no explicit versioning beyond content hash

## References

- `docs/architecture/memory/model.md` — Complete memory model specification
- `crates/types/src/envelope.rs` — RecordEnvelope, EventEnvelope, ArtifactEnvelope definitions
- `crates/core/src/engine.rs` — Apply path with deterministic sort
- `crates/core/tests/sync_tests.rs` — Replay determinism tests
- ADR-003: Sync Protocol (push/pull messages carry EventEnvelope)
- ADR-004: Encryption at Rest (oplog integrity for history preservation)
- ADR-010: Single-Writer Namespace (ownership enforcement)

## Future Work

- Incremental replay from checkpoints (reduce replay latency on reconnect)
- External merge sort for very large event sets
- Conflict resolution policies (pluggable strategies beyond LWW)
- Event pruning (archival of old history while maintaining determinism)
