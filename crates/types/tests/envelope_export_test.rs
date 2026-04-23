use bson::Document;
use rango_types::{
    ArtifactEnvelope, EventEnvelope, GovernanceMetadata, Mutation, MutationMetadata, MutationOp,
    RecordEnvelope, Revision,
};

fn metadata() -> GovernanceMetadata {
    let now = bson::DateTime::now();
    GovernanceMetadata {
        id: "doc-1".to_string(),
        namespace: "test".to_string(),
        tenant_id: "tenant-a".to_string(),
        r#type: "state".to_string(),
        rev: "1000-0-node1".to_string(),
        created_at: now,
        updated_at: now,
        source: "node-1".to_string(),
        actor: "system".to_string(),
        lineage: "lineage-1".to_string(),
        schema_version: 1,
        trust_score: 0.9,
        verified: Some(true),
        expires_at: None,
    }
}

#[test]
fn test_envelope_types_are_exported() {
    let record = RecordEnvelope {
        metadata: metadata(),
        write_id: "write-1".to_string(),
        sequence: 1,
        data: Document::new(),
        conflict_siblings: vec![],
    };
    assert!(record.validate().is_ok());

    let event = EventEnvelope {
        metadata: metadata(),
        write_id: "write-1".to_string(),
        sequence: 1,
        mutation_type: "insert".to_string(),
        mutation_data: None,
        is_tombstone: false,
    };
    assert!(event.validate().is_ok());

    let artifact = ArtifactEnvelope {
        metadata: metadata(),
        write_id: "write-1".to_string(),
        artifact_type: "test_artifact".to_string(),
        source_revision: "1000-0-node1".to_string(),
        content: vec![],
        parent_artifact_revision: None,
    };
    assert!(artifact.validate().is_ok());
}

#[test]
fn test_mutation_metadata_is_exported() {
    let now = bson::DateTime::now();
    let rev = Revision::now("node-1");
    let doc_id = rango_types::DocumentId::new_uuid_v7();
    let mutation = Mutation {
        op: MutationOp::Insert,
        collection: "records".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(Document::new()),
        seq: 1,
        timestamp: now,
        rev: rev.clone(),
        write_id: "write-1".to_string(),
        metadata: MutationMetadata {
            id: doc_id,
            namespace: "records".to_string(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev,
            created_at: now,
            updated_at: now,
            source: "node-1".to_string(),
            actor: "system".to_string(),
            lineage: "lineage-1".to_string(),
            schema_version: 1,
            trust_score: 0.7,
            verified: None,
            expires_at: None,
        },
    };

    assert!(mutation.validate_metadata().is_ok());
}
