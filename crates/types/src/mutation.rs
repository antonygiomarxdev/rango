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
pub struct Mutation {
    pub op: MutationOp,
    pub collection: String,
    pub doc_id: crate::DocumentId,
    pub patch: Option<Document>,
    pub seq: u64,
    pub timestamp: bson::DateTime,
    pub rev: Revision,
    pub write_id: String,
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
}
