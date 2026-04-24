use bson::doc;
use rango_types::{ArtifactEnvelope, GovernanceMetadata, MemoryTier, SemanticReadView};

fn base_metadata() -> GovernanceMetadata {
    let now = bson::DateTime::now();
    GovernanceMetadata {
        id: "semantic-doc".to_string(),
        namespace: "ns-a".to_string(),
        tenant_id: "tenant-a".to_string(),
        r#type: "semantic_projection".to_string(),
        rev: "1000-0-node-a".to_string(),
        created_at: now,
        updated_at: now,
        source: "projection-engine".to_string(),
        actor: "projection-engine".to_string(),
        lineage: "canonical:write-1".to_string(),
        schema_version: 1,
        trust_score: 0.92,
        verified: Some(true),
        expires_at: None,
    }
}

#[test]
fn semantic_projection_envelope_requires_lineage_provenance_and_trust() {
    let envelope = ArtifactEnvelope::new_semantic_projection(
        base_metadata(),
        "semantic-write-1".to_string(),
        "1000-0-node-a".to_string(),
        br#"{"summary":"derived"}"#.to_vec(),
        None,
    )
    .expect("semantic projection constructor should validate metadata");

    assert_eq!(envelope.metadata.r#type, "semantic_projection");
    assert!(!envelope.metadata.lineage.is_empty());
    assert!((0.0..=1.0).contains(&envelope.metadata.trust_score));
    assert_eq!(envelope.artifact_type, "semantic_projection");
}

#[test]
fn semantic_read_view_is_explicitly_derived_and_non_canonical() {
    let view = SemanticReadView::new(MemoryTier::Semantic, doc! { "fact": "answer" });
    assert_eq!(view.tier, MemoryTier::Semantic);
    assert!(view.derived);
    assert!(!view.canonical);
    assert_eq!(view.payload.get_str("fact").unwrap(), "answer");
}
