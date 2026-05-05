# Rango Patterns

Common patterns for persisting state with Rango.

## Pattern 1: Session Memory

Persist user sessions across application restarts.

**When to use:** User sessions, authentication state, temporary user data.

**Rango mapping:** Collection per session type, document per session.

**Governance metadata:**
- `tenant_id`: user_id or "anonymous"
- `lineage`: "session:{session_id}"
- `trust_score`: 1.0 (internal), 0.8 (external)
- `expires_at`: session expiration time

See `../examples/session-memory.{rs,py,ts}` for implementations.

## Pattern 2: Episodic Log

Append-only event history for audit and replay.

**When to use:** Event sourcing, audit trails, action history.

**Rango mapping:** Oplog entries with explicit metadata.

## Pattern 3: State Checkpoint

Periodic snapshots for recovery and rollback.

**When to use:** Long-running processes, game state, ML training checkpoints.

**Rango mapping:** Snapshot units with base sequence and replay window.

## Pattern 4: Configuration

Durable configuration with versioning.

**When to use:** App settings, feature flags, deployment config.

**Rango mapping:** Single-document collection with revision tracking.

## Pattern 5: Multi-tenant Data

Tenant-scoped collections with isolation.

**When to use:** SaaS applications, multi-user systems.

**Rango mapping:** Collection per tenant or namespace, enforced tenant_id in metadata.
