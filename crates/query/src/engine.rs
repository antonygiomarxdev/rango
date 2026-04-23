use bson::{Bson, Document};
use rango_types::RangoError;
use tracing::instrument;

/// Query engine v1 — filters and projections.
pub struct QueryEngine;

impl QueryEngine {
    pub fn new() -> Self {
        Self
    }
}

impl Default for QueryEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply projection to a document.
#[instrument(skip(doc, projection))]
pub fn project(doc: &Document, projection: &Document) -> Result<Document, RangoError> {
    let mut result = Document::new();
    let is_inclusion = projection.values().any(|v| v.as_i32() == Some(1));

    if is_inclusion {
        // Inclusion mode: only include specified fields + _id (unless excluded)
        for (key, value) in projection {
            if value.as_i32() == Some(1) {
                if let Some(v) = doc.get(key) {
                    result.insert(key, v.clone());
                }
            }
        }
        // Always include _id unless explicitly excluded
        if projection.get("_id").and_then(|v| v.as_i32()) != Some(0) {
            if let Some(id) = doc.get("_id") {
                result.insert("_id", id.clone());
            }
        }
    } else {
        // Exclusion mode: copy all fields except excluded
        for (key, value) in doc {
            if projection.get(key).and_then(|v| v.as_i32()) != Some(0) {
                result.insert(key, value.clone());
            }
        }
    }

    Ok(result)
}

/// Extract sort key from a document.
#[instrument(skip(doc))]
pub fn sort_key(doc: &Document, field: &str) -> Result<SortKey, RangoError> {
    match doc.get(field) {
        Some(Bson::Int32(v)) => Ok(SortKey::Int64(*v as i64)),
        Some(Bson::Int64(v)) => Ok(SortKey::Int64(*v)),
        Some(Bson::Double(v)) => Ok(SortKey::Double(*v)),
        Some(Bson::String(v)) => Ok(SortKey::String(v.clone())),
        Some(Bson::DateTime(v)) => Ok(SortKey::DateTime(*v)),
        Some(Bson::Null) => Ok(SortKey::Null),
        None => Ok(SortKey::Null),
        Some(other) => Err(RangoError::InvalidQueryOperator(format!(
            "Cannot sort by field type: {:?}",
            other
        ))),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortKey {
    Null,
    Int64(i64),
    Double(f64),
    String(String),
    DateTime(bson::DateTime),
}

impl Eq for SortKey {}

impl PartialOrd for SortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        fn rank(key: &SortKey) -> u8 {
            match key {
                SortKey::Null => 0,
                SortKey::Int64(_) => 1,
                SortKey::Double(_) => 2,
                SortKey::String(_) => 3,
                SortKey::DateTime(_) => 4,
            }
        }

        match (self, other) {
            (SortKey::Null, SortKey::Null) => Ordering::Equal,
            (SortKey::Int64(a), SortKey::Int64(b)) => a.cmp(b),
            (SortKey::Double(a), SortKey::Double(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (SortKey::String(a), SortKey::String(b)) => a.cmp(b),
            (SortKey::DateTime(a), SortKey::DateTime(b)) => a.cmp(b),
            _ => rank(self).cmp(&rank(other)),
        }
    }
}
