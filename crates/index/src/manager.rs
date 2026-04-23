use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use bson::{Bson, Document};
use rango_types::{CollectionName, IndexDefinition, DocumentId, RangoError};
use tracing::instrument;

/// In-memory secondary index manager.
/// Maps (collection, field) -> sorted map of (value -> [doc_ids]).
#[derive(Debug, Default)]
pub struct IndexManager {
    indexes: Arc<RwLock<IndexMap>>,
}

type IndexMap = std::collections::HashMap<(String, String), BTreeMap<IndexKey, Vec<DocumentId>>>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum IndexKey {
    Null,
    Int64(i64),
    String(String),
}

impl IndexManager {
    pub fn new() -> Self {
        Self::default()
    }

    #[instrument(skip(self, def), fields(collection = %collection.0, field = ?def.fields))]
    pub fn create_index(&self, collection: &CollectionName, def: &IndexDefinition) -> Result<(), RangoError> {
        if def.fields.len() != 1 {
            return Err(RangoError::InvalidQueryOperator(
                "Compound indexes not yet supported".to_string()
            ));
        }
        
        let field = &def.fields[0];
        let mut indexes = self.indexes.write().map_err(|e| RangoError::Storage(e.to_string()))?;
        indexes.insert((collection.0.clone(), field.clone()), BTreeMap::new());
        
        Ok(())
    }

    #[instrument(skip(self), fields(collection = %collection.0, name))]
    pub fn drop_index(&self, collection: &CollectionName, name: &str) -> Result<bool, RangoError> {
        let mut indexes = self.indexes.write().map_err(|e| RangoError::Storage(e.to_string()))?;
        Ok(indexes.remove(&(collection.0.clone(), name.to_string())).is_some())
    }

    #[instrument(skip(self, doc), fields(collection = %collection.0))]
    pub fn index_document(&self, collection: &CollectionName, doc: &Document) -> Result<(), RangoError> {
        let mut indexes = self.indexes.write().map_err(|e| RangoError::Storage(e.to_string()))?;
        
        let doc_id = match doc.get("_id") {
            Some(id) => DocumentId::from_bson(id.clone()),
            None => return Err(RangoError::DocumentNotFound("missing _id".to_string())),
        };
        
        for ((coll, field), tree) in indexes.iter_mut() {
            if coll != &collection.0 {
                continue;
            }
            
            let key = match doc.get(field) {
                Some(Bson::Int32(v)) => IndexKey::Int64(*v as i64),
                Some(Bson::Int64(v)) => IndexKey::Int64(*v),
                Some(Bson::String(v)) => IndexKey::String(v.clone()),
                Some(Bson::Null) | None => IndexKey::Null,
                _ => continue, // Skip unsupported types for indexing
            };
            
            tree.entry(key).or_default().push(doc_id.clone());
        }
        
        Ok(())
    }

    #[instrument(skip(self, _doc), fields(collection = %collection.0))]
    pub fn remove_document(&self, collection: &CollectionName, _doc: &Document) -> Result<(), RangoError> {
        let mut indexes = self.indexes.write().map_err(|e| RangoError::Storage(e.to_string()))?;
        
        for ((coll, _field), tree) in indexes.iter_mut() {
            if coll != &collection.0 {
                continue;
            }
            
            // Simplified removal - clear all entries for this collection
            // In production, would target specific document
            tree.clear();
        }
        
        Ok(())
    }

    #[instrument(skip(self), fields(collection = %collection.0, field))]
    pub fn find_by_index(
        &self,
        collection: &CollectionName,
        field: &str,
        value: &Bson,
    ) -> Result<Vec<DocumentId>, RangoError> {
        let indexes = self.indexes.read().map_err(|e| RangoError::Storage(e.to_string()))?;
        
        let tree = indexes.get(&(collection.0.clone(), field.to_string()))
            .ok_or_else(|| RangoError::InvalidQueryOperator(
                format!("No index on field: {}", field)
            ))?;
        
        let key = match value {
            Bson::Int32(v) => IndexKey::Int64(*v as i64),
            Bson::Int64(v) => IndexKey::Int64(*v),
            Bson::String(v) => IndexKey::String(v.clone()),
            Bson::Null => IndexKey::Null,
            _ => return Ok(vec![]), // Unsupported type
        };
        
        Ok(tree.get(&key).cloned().unwrap_or_default())
    }
}
