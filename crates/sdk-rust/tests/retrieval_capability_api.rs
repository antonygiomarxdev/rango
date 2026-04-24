use rango_sdk::{RetrievalCapabilityRequest, RetrievalStatus};

#[test]
fn retrieval_api_contract_is_typed_and_external_capability_oriented() {
    let request = RetrievalCapabilityRequest::new("tenant-a", "ns-a", "latest outage")
        .with_limit(8);

    assert_eq!(request.tenant_id, "tenant-a");
    assert_eq!(request.namespace, "ns-a");
    assert_eq!(request.query, "latest outage");
    assert_eq!(request.limit, 8);
    assert!(request.vector_limit > 0);
    assert!(request.graph_limit > 0);

    let degraded = RetrievalStatus::Degraded;
    assert!(matches!(degraded, RetrievalStatus::Degraded));
}
