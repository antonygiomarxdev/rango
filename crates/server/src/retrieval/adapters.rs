use bson::doc;
use rango_types::{
    RankingSignals, RetrievalCandidate, RetrievalCapabilityRequest, RetrievalSource,
};

/// Error kind for adapter operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterErrorKind {
    Timeout,
    Unavailable,
    Unauthorized,
    InvalidRequest,
    NotConfigured,
}

/// Error returned by adapter operations.
#[derive(Debug, Clone)]
pub struct AdapterError {
    pub kind: AdapterErrorKind,
    pub reason: String,
}

impl AdapterError {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            kind: AdapterErrorKind::Unavailable,
            reason: reason.into(),
        }
    }

    pub fn not_configured(reason: impl Into<String>) -> Self {
        Self {
            kind: AdapterErrorKind::NotConfigured,
            reason: reason.into(),
        }
    }
}

/// Contract for vector retrieval adapters (Qdrant, pgvector, etc.)
///
/// # Contract
/// 1. **Tenant isolation**: Every query MUST include tenant_id + namespace filters
/// 2. **Timeout**: Operations MUST timeout and return `AdapterErrorKind::Timeout`
/// 3. **Signals**: Returned candidates MUST include `RankingSignals` for ranking
/// 4. **Health check**: `health_check()` MUST return within 1 second
pub trait VectorRetrievalAdapter: Send + Sync {
    /// Query the vector store for candidates matching the request.
    fn query_vector(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError>;

    /// Check if the adapter is healthy and configured.
    fn health_check(&self) -> Result<(), AdapterError> {
        // Default: assume healthy if query succeeds
        Ok(())
    }

    /// Adapter name for observability.
    fn adapter_name(&self) -> &'static str {
        "unknown_vector_adapter"
    }
}

/// Contract for graph retrieval adapters (Neo4j, etc.)
///
/// # Contract
/// 1. **Tenant isolation**: Every query MUST include tenant_id + namespace filters
/// 2. **Parameterized queries**: MUST use parameterized Cypher/SQL (no string concat)
/// 3. **Timeout**: Operations MUST timeout and return `AdapterErrorKind::Timeout`
/// 4. **Signals**: Returned candidates MUST include `RankingSignals` for ranking
/// 5. **Health check**: `health_check()` MUST return within 1 second
pub trait GraphRetrievalAdapter: Send + Sync {
    /// Query the graph store for candidates matching the request.
    fn query_graph(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError>;

    /// Check if the adapter is healthy and configured.
    fn health_check(&self) -> Result<(), AdapterError> {
        // Default: assume healthy if query succeeds
        Ok(())
    }

    /// Adapter name for observability.
    fn adapter_name(&self) -> &'static str {
        "unknown_graph_adapter"
    }
}

// Deterministic mock adapter for tests and fallback-only defaults.
#[derive(Debug, Default, Clone, Copy)]
pub struct AdapterCapabilities;

impl VectorRetrievalAdapter for AdapterCapabilities {
    fn query_vector(
        &self,
        _request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError> {
        Err(AdapterError::unavailable("vector_adapter_unavailable"))
    }

    fn adapter_name(&self) -> &'static str {
        "fallback_vector"
    }
}

impl GraphRetrievalAdapter for AdapterCapabilities {
    fn query_graph(
        &self,
        _request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError> {
        Err(AdapterError::unavailable("graph_adapter_unavailable"))
    }

    fn adapter_name(&self) -> &'static str {
        "fallback_graph"
    }
}

/// Reference Qdrant adapter implementation.
#[derive(Debug, Clone)]
pub struct QdrantAdapter {
    pub available: bool,
}

impl QdrantAdapter {
    pub fn filter_scope(request: &RetrievalCapabilityRequest) -> bson::Document {
        // Tenant and namespace predicates are mandatory on every external query boundary.
        doc! {
            "tenant_id": request.tenant_id.clone(),
            "namespace": request.namespace.clone(),
        }
    }
}

impl VectorRetrievalAdapter for QdrantAdapter {
    fn query_vector(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError> {
        if !self.available {
            return Err(AdapterError::unavailable("qdrant_adapter_unavailable"));
        }

        let _scope = Self::filter_scope(request);
        let candidate = RetrievalCandidate {
            candidate_id: "vector:mock-1".to_string(),
            tenant_id: request.tenant_id.clone(),
            namespace: request.namespace.clone(),
            source: RetrievalSource::Vector,
            lineage: "mock-lineage".to_string(),
            timestamp_ms: bson::DateTime::now().timestamp_millis(),
            payload: doc! { "candidate_id": "vector:mock-1", "text": request.query.clone() },
            signals: RankingSignals {
                relevance: 0.7,
                trust: 0.8,
                recency: 0.6,
                provenance: 0.5,
            },
            score: 0.0,
            explainability: None,
        };
        Ok(vec![candidate])
    }

    fn health_check(&self) -> Result<(), AdapterError> {
        if !self.available {
            return Err(AdapterError::unavailable("qdrant_not_available"));
        }
        Ok(())
    }

    fn adapter_name(&self) -> &'static str {
        "qdrant"
    }
}

/// Reference Neo4j adapter implementation.
#[derive(Debug, Clone)]
pub struct Neo4jAdapter {
    pub available: bool,
}

impl Neo4jAdapter {
    pub fn parameterized_cypher() -> &'static str {
        "MATCH (m:Memory {tenant_id: $tenant_id, namespace: $namespace}) \
         WHERE m.text CONTAINS $query \
         RETURN m.id AS candidate_id, m.text AS text \
         LIMIT $limit"
    }
}

impl GraphRetrievalAdapter for Neo4jAdapter {
    fn query_graph(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError> {
        if !self.available {
            return Err(AdapterError::unavailable("neo4j_adapter_unavailable"));
        }

        let _cypher = Self::parameterized_cypher();
        let candidate = RetrievalCandidate {
            candidate_id: "graph:mock-1".to_string(),
            tenant_id: request.tenant_id.clone(),
            namespace: request.namespace.clone(),
            source: RetrievalSource::Graph,
            lineage: "mock-lineage".to_string(),
            timestamp_ms: bson::DateTime::now().timestamp_millis(),
            payload: doc! { "candidate_id": "graph:mock-1", "text": request.query.clone() },
            signals: RankingSignals {
                relevance: 0.65,
                trust: 0.85,
                recency: 0.55,
                provenance: 0.75,
            },
            score: 0.0,
            explainability: None,
        };
        Ok(vec![candidate])
    }

    fn health_check(&self) -> Result<(), AdapterError> {
        if !self.available {
            return Err(AdapterError::unavailable("neo4j_not_available"));
        }
        Ok(())
    }

    fn adapter_name(&self) -> &'static str {
        "neo4j"
    }
}
