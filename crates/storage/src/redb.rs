use std::path::Path;
use std::sync::Arc;

use bson::Document;
use rango_types::{CollectionName, DocumentId, RangoError};
use redb::{Database, ReadOnlyTable, ReadableDatabase, ReadableTable, Table, TableDefinition};
use tracing::instrument;

use crate::{RangeIter, ScanIter, StorageEngine};

const DOCS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("documents");

#[derive(Debug, Clone)]
pub struct RedbStorage {
    db: Arc<Database>,
}

impl RedbStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RangoError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| RangoError::Storage(e.to_string()))?;
        }
        let db = Database::create(path).map_err(|e| RangoError::Storage(e.to_string()))?;
        {
            let tx = db
                .begin_write()
                .map_err(|e| RangoError::Storage(e.to_string()))?;
            tx.open_table(DOCS_TABLE)
                .map_err(|e| RangoError::Storage(e.to_string()))?;
            tx.commit()
                .map_err(|e| RangoError::Storage(e.to_string()))?;
        }
        Ok(Self { db: Arc::new(db) })
    }

    fn encode_key(collection: &CollectionName, id: &DocumentId) -> Vec<u8> {
        let mut key = collection.0.as_bytes().to_vec();
        key.push(0);
        key.extend_from_slice(format!("{}", id).as_bytes());
        key
    }

    fn collection_prefix(collection: &CollectionName) -> Vec<u8> {
        let mut prefix = collection.0.as_bytes().to_vec();
        prefix.push(0);
        prefix
    }

    fn decode_doc(mut bytes: &[u8]) -> Result<Document, RangoError> {
        Document::from_reader(&mut bytes).map_err(|e| RangoError::Storage(e.to_string()))
    }
}

impl StorageEngine for RedbStorage {
    #[instrument(skip(self), fields(collection = %collection.0))]
    fn get(
        &self,
        collection: &CollectionName,
        id: &DocumentId,
    ) -> Result<Option<Document>, RangoError> {
        let tx = self
            .db
            .begin_read()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let table: ReadOnlyTable<&[u8], &[u8]> = tx
            .open_table(DOCS_TABLE)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let key = Self::encode_key(collection, id);
        match table
            .get(key.as_slice())
            .map_err(|e| RangoError::Storage(e.to_string()))?
        {
            Some(value) => Self::decode_doc(value.value()).map(Some),
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
        let mut bytes = Vec::new();
        doc.to_writer(&mut bytes)
            .map_err(|e| RangoError::Storage(e.to_string()))?;

        let tx = self
            .db
            .begin_write()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        {
            let mut table: Table<&[u8], &[u8]> = tx
                .open_table(DOCS_TABLE)
                .map_err(|e| RangoError::Storage(e.to_string()))?;
            let key = Self::encode_key(collection, id);
            table
                .insert(key.as_slice(), bytes.as_slice())
                .map_err(|e| RangoError::Storage(e.to_string()))?;
        }
        tx.commit()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        Ok(())
    }

    #[instrument(skip(self), fields(collection = %collection.0))]
    fn delete(&self, collection: &CollectionName, id: &DocumentId) -> Result<bool, RangoError> {
        let tx = self
            .db
            .begin_write()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let existed = {
            let mut table: Table<&[u8], &[u8]> = tx
                .open_table(DOCS_TABLE)
                .map_err(|e| RangoError::Storage(e.to_string()))?;
            let key = Self::encode_key(collection, id);
            table
                .remove(key.as_slice())
                .map_err(|e| RangoError::Storage(e.to_string()))?
                .is_some()
        };
        tx.commit()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        Ok(existed)
    }

    #[instrument(skip(self), fields(collection = %collection.0))]
    fn scan(&self, collection: &CollectionName) -> Result<Box<ScanIter>, RangoError> {
        let tx = self
            .db
            .begin_read()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let table: ReadOnlyTable<&[u8], &[u8]> = tx
            .open_table(DOCS_TABLE)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let prefix = Self::collection_prefix(collection);

        let mut docs = Vec::new();
        for kv in table
            .iter()
            .map_err(|e| RangoError::Storage(e.to_string()))?
        {
            let (key, value) = kv.map_err(|e| RangoError::Storage(e.to_string()))?;
            if key.value().starts_with(prefix.as_slice()) {
                docs.push(Self::decode_doc(value.value())?);
            }
        }
        Ok(Box::new(docs.into_iter().map(Ok)))
    }

    #[instrument(skip(self), fields(collection = %collection.0))]
    fn range(
        &self,
        collection: &CollectionName,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
    ) -> Result<Box<RangeIter>, RangoError> {
        let tx = self
            .db
            .begin_read()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let table: ReadOnlyTable<&[u8], &[u8]> = tx
            .open_table(DOCS_TABLE)
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        let prefix = Self::collection_prefix(collection);

        let mut rows = Vec::new();
        for kv in table
            .iter()
            .map_err(|e| RangoError::Storage(e.to_string()))?
        {
            let (key, value) = kv.map_err(|e| RangoError::Storage(e.to_string()))?;
            let key_bytes = key.value();
            if !key_bytes.starts_with(prefix.as_slice()) {
                continue;
            }
            let suffix = &key_bytes[prefix.len()..];
            if let Some(s) = start
                && suffix < s
            {
                continue;
            }
            if let Some(e) = end
                && suffix >= e
            {
                continue;
            }
            rows.push((suffix.to_vec(), Self::decode_doc(value.value())?));
        }

        Ok(Box::new(rows.into_iter().map(Ok)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_path() -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("rango-redb-test-{pid}-{n}.redb"))
    }

    #[test]
    fn redb_put_get_delete_roundtrip() {
        let path = temp_path();
        let storage = RedbStorage::open(&path).expect("open redb");
        let collection = CollectionName::new("users");
        let id = DocumentId::new_uuid_v7();
        let doc = doc! { "name": "alice", "age": 30 };

        storage.put(&collection, &id, &doc).expect("put");
        let found = storage.get(&collection, &id).expect("get").expect("exists");
        assert_eq!(found.get_str("name").ok(), Some("alice"));

        let deleted = storage.delete(&collection, &id).expect("delete");
        assert!(deleted);
        assert!(storage.get(&collection, &id).expect("get").is_none());
    }

    #[test]
    fn redb_scan_is_collection_scoped() {
        let path = temp_path();
        let storage = RedbStorage::open(&path).expect("open redb");
        let users = CollectionName::new("users");
        let orders = CollectionName::new("orders");

        for i in 0..3 {
            storage
                .put(
                    &users,
                    &DocumentId::new_uuid_v7(),
                    &doc! { "kind": "user", "i": i },
                )
                .expect("put user");
            storage
                .put(
                    &orders,
                    &DocumentId::new_uuid_v7(),
                    &doc! { "kind": "order", "i": i },
                )
                .expect("put order");
        }

        let user_docs: Vec<_> = storage.scan(&users).expect("scan users").collect();
        assert_eq!(user_docs.len(), 3);
        assert!(
            user_docs
                .into_iter()
                .all(|r| r.expect("doc").get_str("kind").ok() == Some("user"))
        );
    }

    #[test]
    fn redb_persists_across_reopen() {
        let path = temp_path();
        let collection = CollectionName::new("users");
        let id = DocumentId::new_uuid_v7();
        {
            let storage = RedbStorage::open(&path).expect("open first");
            storage
                .put(&collection, &id, &doc! { "persisted": true })
                .expect("put");
        }
        {
            let storage = RedbStorage::open(&path).expect("open second");
            let found = storage.get(&collection, &id).expect("get").expect("exists");
            assert_eq!(found.get_bool("persisted").ok(), Some(true));
        }
    }
}
