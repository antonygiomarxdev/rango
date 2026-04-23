use bson::Document;
use serde::{Deserialize, Serialize};

/// Version for envelope schema evolution.
pub const RECORD_ENVELOPE_VERSION: u32 = 2;
pub const EVENT_ENVELOPE_VERSION: u32 = 2;
pub const ARTIFACT_ENVELOPE_VERSION: u32 = 2;

/// Canonical governance metadata required on all envelope contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceMetadata {
    pub id: String,
    pub namespace: String,
    pub tenant_id: String,
    pub r#type: String,
    pub rev: String,
    pub created_at: bson::DateTime,
    pub updated_at: bson::DateTime,
    pub source: String,
    pub actor: String,
    pub lineage: String,
    pub schema_version: u32,
    pub trust_score: f64,
    pub verified: Option<bool>,
    pub expires_at: Option<bson::DateTime>,
}

impl GovernanceMetadata {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("id must be non-empty".to_string());
        }
        if self.namespace.is_empty() {
            return Err("namespace must be non-empty".to_string());
        }
        if self.tenant_id.is_empty() {
            return Err("tenant_id must be non-empty".to_string());
        }
        if self.r#type.is_empty() {
            return Err("type must be non-empty".to_string());
        }
        if self.rev.is_empty() {
            return Err("rev must be non-empty".to_string());
        }
        if self.source.is_empty() {
            return Err("source must be non-empty".to_string());
        }
        if self.actor.is_empty() {
            return Err("actor must be non-empty".to_string());
        }
        if self.lineage.is_empty() {
            return Err("lineage must be non-empty".to_string());
        }
        if self.schema_version < 1 {
            return Err("schema_version must be >= 1".to_string());
        }
        if !(0.0..=1.0).contains(&self.trust_score) {
            return Err("trust_score must be between 0.0 and 1.0".to_string());
        }
        if self.updated_at.timestamp_millis() < self.created_at.timestamp_millis() {
            return Err("updated_at must be >= created_at".to_string());
        }
        if let Some(expires_at) = self.expires_at {
            if expires_at.timestamp_millis() < self.created_at.timestamp_millis() {
                return Err("expires_at must be >= created_at".to_string());
            }
        }
        Ok(())
    }
}

/// RecordEnvelope stores current materialized state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordEnvelope {
    pub metadata: GovernanceMetadata,
    pub write_id: String,
    pub sequence: u64,
    pub data: Document,
    pub conflict_siblings: Vec<(String, String)>,
}

impl RecordEnvelope {
    pub fn validate(&self) -> Result<(), String> {
        self.metadata.validate()?;
        if self.write_id.is_empty() {
            return Err("write_id must be non-empty".to_string());
        }
        Ok(())
    }
}

/// EventEnvelope stores append-only history events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub metadata: GovernanceMetadata,
    pub write_id: String,
    pub sequence: u64,
    pub mutation_type: String,
    pub mutation_data: Option<Document>,
    pub is_tombstone: bool,
}

impl EventEnvelope {
    pub fn validate(&self) -> Result<(), String> {
        self.metadata.validate()?;
        if self.write_id.is_empty() {
            return Err("write_id must be non-empty".to_string());
        }
        if self.mutation_type.is_empty() {
            return Err("mutation_type must be non-empty".to_string());
        }
        Ok(())
    }
}

/// ArtifactEnvelope stores derived, rebuildable artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEnvelope {
    pub metadata: GovernanceMetadata,
    pub write_id: String,
    pub artifact_type: String,
    pub source_revision: String,
    pub content: Vec<u8>,
    pub parent_artifact_revision: Option<String>,
}

impl ArtifactEnvelope {
    pub fn validate(&self) -> Result<(), String> {
        self.metadata.validate()?;
        if self.write_id.is_empty() {
            return Err("write_id must be non-empty".to_string());
        }
        if self.artifact_type.is_empty() {
            return Err("artifact_type must be non-empty".to_string());
        }
        if self.source_revision.is_empty() {
            return Err("source_revision must be non-empty".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_metadata() -> GovernanceMetadata {
        let now = bson::DateTime::now();
        GovernanceMetadata {
            id: "doc-123".to_string(),
            namespace: "test-namespace".to_string(),
            tenant_id: "tenant-1".to_string(),
            r#type: "state".to_string(),
            rev: "1000-0-node1".to_string(),
            created_at: now,
            updated_at: now,
            source: "node-1".to_string(),
            actor: "system".to_string(),
            lineage: "lineage-001".to_string(),
            schema_version: 1,
            trust_score: 0.8,
            verified: Some(true),
            expires_at: None,
        }
    }

    #[test]
    fn test_governance_metadata_validation() {
        let mut metadata = base_metadata();
        assert!(metadata.validate().is_ok());

        metadata.tenant_id.clear();
        assert!(metadata.validate().is_err());

        let mut metadata = base_metadata();
        metadata.trust_score = 1.2;
        assert!(metadata.validate().is_err());
    }

    #[test]
    fn test_record_envelope_validation() {
        let valid = RecordEnvelope {
            metadata: base_metadata(),
            write_id: "write-001".to_string(),
            sequence: 1,
            data: Document::new(),
            conflict_siblings: vec![],
        };
        assert!(valid.validate().is_ok());

        let mut invalid = valid.clone();
        invalid.metadata.namespace = String::new();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_event_envelope_validation() {
        let valid = EventEnvelope {
            metadata: base_metadata(),
            write_id: "write-001".to_string(),
            sequence: 1,
            mutation_type: "insert".to_string(),
            mutation_data: Some(Document::new()),
            is_tombstone: false,
        };
        assert!(valid.validate().is_ok());

        let mut invalid = valid.clone();
        invalid.mutation_type.clear();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_artifact_envelope_validation() {
        let valid = ArtifactEnvelope {
            metadata: base_metadata(),
            write_id: "write-001".to_string(),
            artifact_type: "summary".to_string(),
            source_revision: "1000-0-node1".to_string(),
            content: vec![],
            parent_artifact_revision: None,
        };
        assert!(valid.validate().is_ok());

        let mut invalid = valid.clone();
        invalid.source_revision.clear();
        assert!(invalid.validate().is_err());
    }
}
