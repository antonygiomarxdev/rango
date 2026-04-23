use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, RwLock};

use bson::Document;
use rango_types::{CollectionName, DocumentId, RangoError};
use tracing::instrument;

use crate::StorageEngine;

/// In-memory storage backend for testing and development.
/// Not suitable for production — data is lost when process exits.
#[derive(Debug, Clone, Default)]
pub struct MemoryStorage {
    data: Arc<RwLock<HashMap<String, HashMap<Vec<u8>, Vec<u8>>>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }

    fn encode_key(collection: &CollectionName, id: &DocumentId) -> Vec<u8> {
        let mut key = collection.0.as_bytes().to_vec();
        key.push(0); // null separator
        // Use a simple string representation for the ID in keys
        let id_bytes = format!("{}", id);
        key.extend_from_slice(id_bytes.as_bytes());
        key
    }
}

impl StorageEngine for MemoryStorage {
    #[instrument(skip(self), fields(collection = %collection.0))]
    fn get(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
    ) -> Result<Option<Document>, RangoError> {
        let data = self.data.read().map_err(|e| RangoError::Storage(e.to_string()))?;
        let coll = data.get(&collection.0);
        
        match coll {
            Some(coll_data) => {
                let key = Self::encode_key(collection, id);
                match coll_data.get(&key) {
                    Some(bytes) => {
                        let doc = Document::from_reader(&mut Cursor::new(bytes))
                            .map_err(|e| RangoError::Storage(e.to_string()))?;
                        Ok(Some(doc))
                    }
                    None => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self, doc), fields(collection = %collection.0))]
    fn put(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
        doc: &Document,
    ) -> Result<(), RangoError> {
        let mut data = self.data.write().map_err(|e| RangoError::Storage(e.to_string()))?;
        let coll = data.entry(collection.0.clone()).or_default();
        
        let key = Self::encode_key(collection, id);
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        coll.insert(key, bytes);
        
        Ok(())
    }

    #[instrument(skip(self), fields(collection = %collection.0))]
    fn delete(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
    ) -> Result<bool, RangoError> {
        let mut data = self.data.write().map_err(|e| RangoError::Storage(e.to_string()))?;
        
        match data.get_mut(&collection.0) {
            Some(coll_data) => {
                let key = Self::encode_key(collection, id);
                Ok(coll_data.remove(&key).is_some())
            }
            None => Ok(false),
        }
    }

    #[instrument(skip(self), fields(collection = %collection.0))]
    fn scan(
        &self,
        collection: &CollectionName,
    ) -> Result<Box<dyn Iterator<Item = Result<Document, RangoError>>>, RangoError> {
        let data = self.data.read().map_err(|e| RangoError::Storage(e.to_string()))?;
        let coll_data = data.get(&collection.0).cloned();
        
        let iter = MemoryScanIter {
            items: coll_data.map(|c| c.values().cloned().collect()).unwrap_or_default(),
            index: 0,
        };
        
        Ok(Box::new(iter))
    }

    #[instrument(skip(self), fields(collection = %_collection.0))]
    fn range(
        &self,
        _collection: &CollectionName,
        _start: Option<&[u8]>,
        _end: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Document), RangoError>>>, RangoError> {
        Ok(Box::new(std::iter::empty()))
    }
}

struct MemoryScanIter {
    items: Vec<Vec<u8>>,
    index: usize,
}

impl Iterator for MemoryScanIter {
    type Item = Result<Document, RangoError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.items.len() {
            return None;
        }
        
        let bytes = &self.items[self.index];
        self.index += 1;
        
        Some(
            Document::from_reader(&mut Cursor::new(bytes))
                .map_err(|e| RangoError::Storage(e.to_string()))
        )
    }
}
