use crate::Revision;
use bson::Document;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationOp {
    Insert,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationMetadata {
    pub id: crate::DocumentId,
    pub namespace: String,
    pub tenant_id: String,
    pub r#type: String,
    pub rev: Revision,
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

impl MutationMetadata {
    pub fn validate(&self) -> Result<(), String> {
        if self.namespace.is_empty() {
            return Err("namespace must be non-empty".to_string());
        }
        if self.tenant_id.is_empty() {
            return Err("tenant_id must be non-empty".to_string());
        }
        if self.r#type.is_empty() {
            return Err("type must be non-empty".to_string());
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
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    pub op: MutationOp,
    pub collection: String,
    pub doc_id: crate::DocumentId,
    pub patch: Option<Document>,
    pub seq: u64,
    pub timestamp: bson::DateTime,
    pub rev: Revision,
    pub write_id: String,
    pub metadata: MutationMetadata,
}

impl Mutation {
    pub fn write_id(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.op.hash(&mut hasher);
        self.collection.hash(&mut hasher);
        self.doc_id.hash(&mut hasher);
        if let Some(patch) = &self.patch {
            patch.hash(&mut hasher);
        }
        self.rev.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    pub fn validate_metadata(&self) -> Result<(), String> {
        self.metadata.validate()
    }
}
