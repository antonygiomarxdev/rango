# Rango Integration Audit Checklist

Review existing Rango integrations against these criteria.

## Critical

- [ ] **StorageEngine generic erased** (Rust only)
  - `RangoClient` should not be generic over `StorageEngine`
  - Use `Arc<dyn StorageEngine>` at initialization
  
- [ ] **Canonical metadata present in all writes**
  - Every document has: `tenant_id`, `lineage`, `trust_score`, `verified`
  - Check `GovernanceMetadata` usage

- [ ] **write_id used for idempotency**
  - All mutations generate and check `write_id`
  - Duplicate write_ids are rejected or deduplicated

## Important

- [ ] **tenant_id enforced for multi-tenant data**
  - No hardcoded "default" tenant in production paths
  - Tenant isolation verified in tests

- [ ] **Governance hooks wired**
  - Validation hooks run before writes
  - Read path enforces trust thresholds
  - Promotion path explicit for semantic memory

- [ ] **Oplog append after storage put**
  - Storage write succeeds before oplog entry
  - Atomicity handled or documented

- [ ] **Error handling for storage exhaustion**
  - `DegradingStorage` or equivalent used
  - Graceful degradation tested

## Minor

- [ ] **Sync configuration documented**
  - Sync frequency, batch size, conflict resolution defined
  - Multi-node scenarios considered

- [ ] **Metrics and observability**
  - OpenTelemetry metrics wired (if applicable)
  - Health endpoints accessible (if server)

- [ ] **Documentation**
  - SDK stability contract documented
  - Breaking change policy followed
