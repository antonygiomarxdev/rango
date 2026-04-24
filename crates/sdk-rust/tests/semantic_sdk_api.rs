use bson::doc;
use rango_sdk::{DerivedReadLabel, SemanticProjectionRequest, SemanticProjectionResponse};
use rango_types::MemoryTier;

#[test]
fn semantic_sdk_contract_is_typed_and_non_canonical() {
    let request = SemanticProjectionRequest::new(
        "tenant-a",
        "ns-a",
        "candidate-1",
        doc! { "summary": "derived semantic memory" },
    );

    assert_eq!(request.tenant_id, "tenant-a");
    assert_eq!(request.namespace, "ns-a");
    assert_eq!(request.candidate_id, "candidate-1");
    assert_eq!(request.tier, MemoryTier::Semantic);

    let response = SemanticProjectionResponse {
        accepted: true,
        write_id: "semantic-write-1".to_string(),
        label: DerivedReadLabel::derived_non_canonical(),
    };

    assert!(response.accepted);
    assert!(response.label.derived);
    assert!(!response.label.canonical);
}
