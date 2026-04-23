use bson::Document;
use rango_types::{CollectionName, DocumentId, RangoError};

pub type ScanIter = dyn Iterator<Item = Result<Document, RangoError>>;
pub type RangeIter = dyn Iterator<Item = Result<(Vec<u8>, Document), RangoError>>;

/// Core storage operations — backend-agnostic.
pub trait StorageEngine: Send + Sync {
    fn get(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
    ) -> Result<Option<Document>, RangoError>;

    fn put(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
        doc: &Document,
    ) -> Result<(), RangoError>;

    fn delete(&self, collection: &CollectionName, id: &DocumentId) -> Result<bool, RangoError>;

    fn scan(&self, collection: &CollectionName) -> Result<Box<ScanIter>, RangoError>;

    fn range(
        &self,
        collection: &CollectionName,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<RangeIter>, RangoError>;
}

/// Transactional storage operations — optional depending on backend capabilities.
pub trait TransactionalStorage: StorageEngine {
    type Tx;

    fn begin_tx(&self) -> Result<Self::Tx, RangoError>;
    fn commit_tx(&self, tx: Self::Tx) -> Result<(), RangoError>;
    fn rollback_tx(&self, tx: Self::Tx) -> Result<(), RangoError>;
}
