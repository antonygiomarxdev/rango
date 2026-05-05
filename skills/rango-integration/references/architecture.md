# Rango Architecture Guidelines

## What to Persist (Canonical State)

- User data and documents
- Application state that must survive restarts
- Audit trails and governance decisions
- Configuration and settings
- Checkpoints and snapshots

## What NOT to Persist (Derived/Ephemeral)

- Temporary computations and caches
- UI state and view models
- Derived projections (use Rango as source of truth, compute on read)
- Large binary blobs (store metadata in Rango, blobs in S3/filesystem)

## Collection Design

### Granularity
- One collection per entity type (e.g., "users", "orders", "sessions")
- Avoid mega-collections with mixed entity types

### Naming
- Use snake_case: `user_sessions`, `audit_trail`
- Prefix system collections: `__governance`, `__checkpoints`

## Metadata Requirements

Every canonical write MUST include:
- `tenant_id`: Scope for multi-tenant isolation
- `lineage`: Provenance chain (e.g., "user:123:action")
- `trust_score`: 0.0-1.0, 1.0 for verified internal, lower for external
- `verified`: Boolean, true if source is authenticated
- `expires_at`: Optional TTL

## Sync Strategy

- Local-first: writes are local, sync is background
- Use `write_id` for idempotency
- Handle conflicts with LWW (last-write-wins) or custom logic
- Sync frequency: batch every N seconds or M mutations

## Projection Boundaries

Rango stores canonical truth. Derived data:
- Vector embeddings: store in pgvector/Qdrant, reference canonical_id
- Search indexes: rebuild from Rango state
- Aggregations: compute on demand or cache ephemerally
