use bson::Document;
use serde::{Deserialize, Serialize};

/// A Rango document with reserved metadata fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangoDocument {
    /// User data + reserved fields (_id, _rev, _updated_at, _deleted, _source_node)
    #[serde(flatten)]
    pub data: Document,
}

impl RangoDocument {
    pub fn id(&self) -> Option<&bson::Bson> {
        self.data.get("_id")
    }

    pub fn revision(&self) -> Option<i64> {
        self.data.get_i64("_rev").ok()
    }

    pub fn updated_at(&self) -> Option<bson::DateTime> {
        self.data.get_datetime("_updated_at").ok().copied()
    }

    pub fn is_deleted(&self) -> bool {
        self.data.get_bool("_deleted").unwrap_or(false)
    }

    pub fn source_node(&self) -> Option<&str> {
        self.data.get_str("_source_node").ok()
    }
}
