# ADR-009: Core/Extension Boundary and Domain Neutrality

## Status
Accepted

## Context

Phase 8 repositions Rango from a generic data engine into a **memory-first local engine for stateful AI workloads**. However, this positioning must NOT leak AI-specific semantics into the core engine.

**Problem:** Without a frozen architectural boundary, core crates (types, core, storage, sync, server) risk accumulating:
- AI-specific vocabulary (e.g., "agent", "memory", "conversation", "tool")
- Agent-specific mutation semantics
- LLM-centric conflict policies
- Stateful AI callbacks or hooks

This couples Rango tightly to one AI architecture and makes it harder to support:
- Other stateful workloads (time-series sensors, IoT edge processing, field robotics)
- Alternative AI frameworks or approaches
- Future evolution of AI capabilities

**Goal:** Freeze the core as a **domain-neutral local-first engine** and put all AI-specific behavior behind explicit extension contracts.

## Decision

**Core crates remain domain-neutral:**

âœ… `crates/types` â€” Document storage, mutation, envelope contracts (no AI terms)
âœ… `crates/core` â€” CRUD, querying, local transactions (no AI terms)
âœ… `crates/storage` â€” Physical storage abstraction, WAL, recovery (no AI terms)
âœ… `crates/index` â€” Indexing, statistics (no AI terms)
âœ… `crates/query` â€” Filter/projection/sort/aggregation (no AI terms)
âœ… `crates/oplog` â€” Append-only history, dedup, idempotency (no AI terms)
âœ… `crates/sync` â€” Push/pull, checkpoints, conflict resolution (no AI terms)
âœ… `crates/server` â€” HTTP routes, auth, validation (no AI terms)

**AI-specific behavior lives in extensions:**

âœ… `examples/` â€” Wedge demos, sample integrations (AI terms OK here)
âœ… Extension contracts â€” Trait interfaces for AI-specific derivation
âœ… SDK documentation â€” Usage patterns for AI workloads

## Rationale

### Why Domain Neutrality?

1. **Durability:** Core semantics don't change if AI frameworks evolve
2. **Composability:** Other non-AI stateful workloads can build on same core
3. **Portability:** Core is reusable across AI platforms, edge devices, IoT
4. **Testability:** Core tests don't require AI context or mocking
5. **Security:** Easier to audit and verify core without domain baggage

### Why Extensions?

Rango's value for AI comes from **operational characteristics** (local-first, offline-writes, incremental sync), not from baked-in AI logic. Extensions encapsulate AI-specific behavior:

- **Conversation buffer management** â†’ Extension consumes EventEnvelope stream, produces ArtifactEnvelope
- **Tool call history** â†’ Extension tracks tool semantics; core tracks mutations
- **Agent state checkpointing** â†’ Extension serializes agent state into ArtifactEnvelope
- **Memory embedding** â†’ Extension derives embeddings from RecordEnvelope; core stores opaquely

**Benefit:** Core changes minimally; extensions adapt to new AI techniques.

### Example: Avoiding AI Terms in Core

**âŒ Bad (AI leaked into core):**
```rust
// crates/core/src/engine.rs
pub struct RangoEngine {
    pub agent_id: String,
    pub conversation_history: Vec<Message>,
    pub memory_buffer: Vec<u8>,
    pub tool_registry: HashMap<String, Tool>,
}
```

**âœ… Good (domain-neutral core):**
```rust
// crates/core/src/engine.rs
pub struct RangoEngine {
    // No AI-specific fields; only storage, sync, transactions
}

// AI integration lives in extension
pub trait AgentMemoryDeriver {
    fn derive_from_record(&self, record: &RecordEnvelope) -> Result<ArtifactEnvelope>;
    fn derive_from_history(&self, events: &[EventEnvelope]) -> Result<ArtifactEnvelope>;
}
```

## Consequences

### Positive

- **Core remains simple:** No AI coupling; easier to reason about, test, optimize
- **Extensibility:** Future AI frameworks (e.g., o1, o3) inherit from same core without modification
- **Broader adoption:** Non-AI stateful workloads (IoT, monitoring, field robotics) can use same engine
- **Maintenance:** Fewer breaking changes in core as AI landscape evolves
- **Security:** Smaller attack surface in core; AI-specific features isolated in extensions

### Negative

- **Verbosity:** AI workloads must implement extension traits instead of calling built-in APIs
- **Discoverability:** Extension patterns not discoverable in core API docs
- **Fragmentation:** Multiple incompatible extension implementations possible (coordination needed)

## Implementation

### Forbidden Terms in Core Crates

The following terms are **not permitted** in `crates/{types,core,storage,index,query,oplog,sync,server}`:

- âŒ agent, actor, bot
- âŒ conversation, dialog, chat, message
- âŒ memory, buffer, cache (in semantic context)
- âŒ tool, function_call, skill
- âŒ model, embedding, vector (in embedding context)
- âŒ llm, ai, neural, learning

**Exceptions:**
- Comments/docs explaining how core is used by AI workloads (OK)
- Generic terms like "buffer" in buffer pool context (OK)
- Metrics/observability naming (optional; recommended to keep generic)

### Extension Trait Pattern

```rust
// Extension lives OUTSIDE core (e.g., examples/ or separate crate)
pub trait StateDeriver {
    /// Derive application-specific artifact from current record state.
    fn derive_artifact(
        &self,
        record: &RecordEnvelope,
        context: &DerivationContext,
    ) -> Result<ArtifactEnvelope>;
    
    /// Optionally derive from history for incremental updates.
    fn derive_from_history(
        &self,
        events: &[EventEnvelope],
        last_artifact: Option<&ArtifactEnvelope>,
    ) -> Result<ArtifactEnvelope>;
}

// Application implements extension and calls into core
struct MyStatefulWorkload;

impl StateDeriver for MyStatefulWorkload {
    fn derive_artifact(&self, record: &RecordEnvelope, _ctx: &DerivationContext) -> Result<ArtifactEnvelope> {
        // AI-specific logic here (e.g., memory management, embedding, etc.)
        // Uses only RecordEnvelope and EventEnvelope contracts from core
        Ok(artifact)
    }
}
```

### Code Review Checklist

Before merging PRs to core crates:

- [ ] No forbidden AI terms in code (comments/docs are OK)
- [ ] No new fields added to RecordEnvelope/EventEnvelope for AI purposes
- [ ] No new enum variants for AI mutation types
- [ ] All new features are applicable to non-AI stateful workloads
- [ ] Tests don't depend on AI context

### Documentation

- `docs/architecture/memory/model.md` â€” Core envelope contracts (domain-neutral)
- `docs/extension-patterns.md` â€” Example extension trait and usage patterns (NEW)
- `examples/stateful_agent_wedge.rs` â€” AI workload example using core + extension

## Non-Goals

- **AI SDK in core:** Rango core stays as storage engine, not AI framework
- **Pre-built agents:** Agents built by applications, not shipped in core
- **Tool registry:** Tool semantics driven by applications/extensions, not core
- **Automatic conflict resolution policies:** Core logs conflicts; resolution is policy-driven (AI or otherwise)

## References

- `docs/architecture/memory/model.md` â€” RecordEnvelope, EventEnvelope contracts (domain-neutral)
- `crates/types/src/envelope.rs` â€” Type definitions
- ADR-008: State/History Separation (deterministic replay foundation)
- ADR-010: Single-Writer Namespace (ownership enforcement, works for any workload)

## Future Work

- `docs/extension-patterns.md` â€” Best practices for writing extensions
- Example extensions for common AI patterns (memory derivation, embedding, conflict resolution)
- Extension discovery/registration mechanism (optional registry)
- Versioned extension trait protocol (for forward compatibility)

