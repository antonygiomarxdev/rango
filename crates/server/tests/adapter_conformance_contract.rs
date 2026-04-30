use rango_server::retrieval::adapters::{
    AdapterCapabilities, AdapterErrorKind, GraphRetrievalAdapter, Neo4jAdapter, QdrantAdapter,
    VectorRetrievalAdapter,
};
use rango_types::RetrievalCapabilityRequest;

fn request() -> RetrievalCapabilityRequest {
    RetrievalCapabilityRequest::new("tenant-a", "ns1", "test query")
}

/// Contract test: All adapters MUST include tenant_id and namespace in scope.
#[tokio::test]
async fn vector_adapter_includes_tenant_and_namespace_scope() {
    let _adapter = QdrantAdapter { available: true };
    let req = request();
    let scope = QdrantAdapter::filter_scope(&req);
    assert_eq!(scope.get_str("tenant_id").unwrap(), "tenant-a");
    assert_eq!(scope.get_str("namespace").unwrap(), "ns1");
}

/// Contract test: Graph adapter uses parameterized queries (no string concat).
#[tokio::test]
async fn graph_adapter_uses_parameterized_cypher() {
    let cypher = Neo4jAdapter::parameterized_cypher();
    assert!(
        cypher.contains("$tenant_id"),
        "Cypher must use parameterized tenant_id"
    );
    assert!(
        cypher.contains("$namespace"),
        "Cypher must use parameterized namespace"
    );
    assert!(
        cypher.contains("$query"),
        "Cypher must use parameterized query"
    );
    assert!(
        cypher.contains("$limit"),
        "Cypher must use parameterized limit"
    );
    // Ensure no string concatenation patterns
    assert!(
        !cypher.contains("'"),
        "Cypher must not contain single quotes (string concat risk)"
    );
}

/// Contract test: Adapter returns candidates with required RankingSignals.
#[tokio::test]
async fn vector_adapter_returns_ranking_signals() {
    let adapter = QdrantAdapter { available: true };
    let candidates = adapter.query_vector(&request()).unwrap();
    assert!(!candidates.is_empty());
    for candidate in candidates {
        assert!(
            candidate.signals.relevance > 0.0,
            "relevance signal must be present"
        );
        assert!(candidate.signals.trust > 0.0, "trust signal must be present");
        assert!(
            candidate.signals.recency > 0.0,
            "recency signal must be present"
        );
        assert!(
            candidate.signals.provenance > 0.0,
            "provenance signal must be present"
        );
    }
}

/// Contract test: Adapter returns candidates with tenant scoping.
#[tokio::test]
async fn vector_adapter_returns_tenant_scoped_candidates() {
    let adapter = QdrantAdapter { available: true };
    let candidates = adapter.query_vector(&request()).unwrap();
    for candidate in candidates {
        assert_eq!(candidate.tenant_id, "tenant-a");
        assert_eq!(candidate.namespace, "ns1");
    }
}

/// Contract test: Unavailable adapter returns Unavailable error.
#[tokio::test]
async fn unavailable_vector_adapter_returns_unavailable() {
    let adapter = QdrantAdapter { available: false };
    let result = adapter.query_vector(&request());
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind, AdapterErrorKind::Unavailable);
}

/// Contract test: Unavailable graph adapter returns Unavailable error.
#[tokio::test]
async fn unavailable_graph_adapter_returns_unavailable() {
    let adapter = Neo4jAdapter { available: false };
    let result = adapter.query_graph(&request());
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind, AdapterErrorKind::Unavailable);
}

/// Contract test: Health check reflects availability.
#[tokio::test]
async fn health_check_reflects_availability() {
    let healthy_vector = QdrantAdapter { available: true };
    assert!(healthy_vector.health_check().is_ok());

    let unhealthy_vector = QdrantAdapter { available: false };
    assert!(unhealthy_vector.health_check().is_err());

    let healthy_graph = Neo4jAdapter { available: true };
    assert!(healthy_graph.health_check().is_ok());

    let unhealthy_graph = Neo4jAdapter { available: false };
    assert!(unhealthy_graph.health_check().is_err());
}

/// Contract test: Fallback adapter returns Unavailable.
#[tokio::test]
async fn fallback_adapter_returns_unavailable() {
    let fallback = AdapterCapabilities;
    let vector_result = fallback.query_vector(&request());
    let graph_result = fallback.query_graph(&request());

    assert!(vector_result.is_err());
    assert_eq!(vector_result.unwrap_err().kind, AdapterErrorKind::Unavailable);

    assert!(graph_result.is_err());
    assert_eq!(graph_result.unwrap_err().kind, AdapterErrorKind::Unavailable);
}

/// Contract test: Adapter names are descriptive.
#[tokio::test]
async fn adapter_names_are_descriptive() {
    assert_eq!(QdrantAdapter { available: true }.adapter_name(), "qdrant");
    assert_eq!(Neo4jAdapter { available: true }.adapter_name(), "neo4j");
    assert_eq!(
        <AdapterCapabilities as VectorRetrievalAdapter>::adapter_name(&AdapterCapabilities),
        "fallback_vector"
    );
}
