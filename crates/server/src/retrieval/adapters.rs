use bson::doc;
use rango_types::{
    RankingSignals, RetrievalCandidate, RetrievalCapabilityRequest, RetrievalSource,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterErrorKind {
    Timeout,
    Unavailable,
    Unauthorized,
    InvalidRequest,
}

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
}

pub trait VectorRetrievalAdapter: Send + Sync {
    fn query_vector(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError>;
}

pub trait GraphRetrievalAdapter: Send + Sync {
    fn query_graph(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError>;
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
}

impl GraphRetrievalAdapter for AdapterCapabilities {
    fn query_graph(
        &self,
        _request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError> {
        Err(AdapterError::unavailable("graph_adapter_unavailable"))
    }
}

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
}

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
}
