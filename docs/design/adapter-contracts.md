# Adapter Capability Contracts

## Overview

Rango supports pluggable retrieval adapters for vector and graph stores. This document defines the contract that all adapters MUST implement to ensure interoperability, security, and reliability.

## Contracts

### VectorRetrievalAdapter

```rust
pub trait VectorRetrievalAdapter: Send + Sync {
    fn query_vector(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError>;

    fn health_check(&self) -> Result<(), AdapterError>;
    fn adapter_name(&self) -> &'static str;
}
```

#### Requirements

1. **Tenant Isolation**: Every query MUST filter by `tenant_id` and `namespace`
2. **Timeout**: Operations MUST timeout and return `AdapterErrorKind::Timeout`
3. **Signals**: All candidates MUST include `RankingSignals` (relevance, trust, recency, provenance)
4. **Health Check**: MUST return within 1 second; reflects actual connectivity
5. **Name**: MUST return a descriptive static string for observability

### GraphRetrievalAdapter

```rust
pub trait GraphRetrievalAdapter: Send + Sync {
    fn query_graph(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError>;

    fn health_check(&self) -> Result<(), AdapterError>;
    fn adapter_name(&self) -> &'static str;
}
```

#### Requirements

1. **Tenant Isolation**: Every query MUST filter by `tenant_id` and `namespace`
2. **Parameterized Queries**: MUST use parameterized Cypher/SQL (no string concatenation)
3. **Timeout**: Operations MUST timeout and return `AdapterErrorKind::Timeout`
4. **Signals**: All candidates MUST include `RankingSignals`
5. **Health Check**: MUST return within 1 second
6. **Name**: MUST return a descriptive static string

## Error Handling

| Error Kind | When to Use |
|-----------|-------------|
| `Timeout` | Query exceeded configured timeout |
| `Unavailable` | Adapter not configured or service down |
| `Unauthorized` | Credentials invalid or expired |
| `InvalidRequest` | Query parameters malformed |
| `NotConfigured` | Adapter missing required configuration |

## Reference Implementations

| Adapter | Type | Status |
|---------|------|--------|
| `QdrantAdapter` | Vector | Mock (ready for real implementation) |
| `Neo4jAdapter` | Graph | Mock (ready for real implementation) |
| `AdapterCapabilities` | Both | Fallback (always returns Unavailable) |

## Conformance Tests

Run the conformance suite:

```bash
cargo test -p rango-server --test adapter_conformance_contract
```

Tests verify:
- Tenant/namespace scoping
- Parameterized queries (no injection risk)
- Ranking signals presence
- Error kind correctness
- Health check behavior
- Adapter naming
