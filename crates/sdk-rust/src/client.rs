use rango_core::RangoEngine;
use rango_oplog::Oplog;
use rango_storage::StorageEngine;
use rango_types::*;
use bson::Document;
use std::sync::Arc;
use tracing::instrument;

/// Public SDK for Rango.
pub struct RangoClient<S: StorageEngine> {
    pub engine: RangoEngine<S>,
}

impl<S: StorageEngine> RangoClient<S> {
    #[instrument(skip(storage, oplog, node_id))]
    pub fn open(
        storage: Arc<S>,
        oplog: Arc<dyn Oplog>,
        node_id: impl Into<String>,
    ) -> Result<Self, RangoError> {
        Ok(Self {
            engine: RangoEngine::open(storage, oplog, node_id)?,
        })
    }

    #[instrument(skip(storage, oplog, node_id, config))]
    pub fn open_with_config(
        storage: Arc<S>,
        oplog: Arc<dyn Oplog>,
        node_id: impl Into<String>,
        config: RangoConfig,
    ) -> Result<Self, RangoError> {
        Ok(Self {
            engine: RangoEngine::open_with_config(storage, oplog, node_id, config)?,
        })
    }

    pub fn collection(&self, name: &str) -> CollectionClient<S> {
        CollectionClient {
            client: self,
            name: CollectionName::new(name),
        }
    }
}

pub struct CollectionClient<'a, S: StorageEngine> {
    client: &'a RangoClient<S>,
    name: CollectionName,
}

impl<S: StorageEngine> CollectionClient<'_, S> {
    #[instrument(skip(self, doc), fields(collection = %self.name.0))]
    pub fn insert_one(&self, doc: Document) -> Result<DocumentId, RangoError> {
        self.client.engine.insert_one(&self.name, doc)
    }

    #[instrument(skip(self), fields(collection = %self.name.0))]
    pub fn find_one(&self, id: &DocumentId) -> Result<Option<RangoDocument>, RangoError> {
        self.client.engine.find_one(&self.name, id)
    }

    #[instrument(skip(self), fields(collection = %self.name.0))]
    pub fn find_many(&self) -> Result<rango_core::Cursor, RangoError> {
        self.client.engine.find_many(&self.name)
    }

    #[instrument(skip(self, update), fields(collection = %self.name.0))]
    pub fn update_one(&self, id: &DocumentId, update: Document) -> Result<bool, RangoError> {
        self.client.engine.update_one(&self.name, id, update)
    }

    #[instrument(skip(self), fields(collection = %self.name.0))]
    pub fn delete_one(&self, id: &DocumentId) -> Result<bool, RangoError> {
        self.client.engine.delete_one(&self.name, id)
    }
}
