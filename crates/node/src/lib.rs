use std::path::Path;
use std::sync::Arc;

use bson::Document;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use rango_oplog::FileOplog;
use rango_storage::{DegradingStorage, RedbStorage};
use rango_types::{DocumentId, RangoDocument, RangoError};

/// Convert a JSON string to a bson::Document.
fn json_str_to_document(json_str: &str) -> Result<Document> {
    let json_value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| Error::new(Status::InvalidArg, format!("JSON parse error: {e}")))?;
    let doc: Document = serde_json::from_value(json_value)
        .map_err(|e| Error::new(Status::InvalidArg, format!("BSON conversion error: {e}")))?;
    Ok(doc)
}

/// Convert a bson::Document to a JSON string.
fn document_to_json_str(doc: &Document) -> Result<String> {
    let json_value = serde_json::to_value(doc).map_err(|e| {
        Error::new(
            Status::GenericFailure,
            format!("JSON serialization error: {e}"),
        )
    })?;
    serde_json::to_string(&json_value)
        .map_err(|e| Error::new(Status::GenericFailure, format!("JSON stringify error: {e}")))
}

/// Convert RangoDocument to JSON string with _id and _rev fields.
fn rango_doc_to_json(doc: RangoDocument) -> Result<String> {
    let mut doc_data = doc.data.clone();
    if let Some(id) = doc.id() {
        doc_data.insert("_id", id.to_string());
    }
    if let Some(rev) = doc.revision() {
        doc_data.insert("_rev", rev);
    }
    document_to_json_str(&doc_data)
}

/// Parse a document ID string into a DocumentId.
fn parse_doc_id(id: &str) -> DocumentId {
    // Try UUID first
    if let Ok(uuid) = uuid::Uuid::parse_str(id) {
        return DocumentId::from_bson(bson::Bson::Binary(bson::Binary {
            subtype: bson::spec::BinarySubtype::Uuid,
            bytes: uuid.as_bytes().to_vec(),
        }));
    }
    // Try ObjectId
    if let Ok(oid) = bson::oid::ObjectId::parse_str(id) {
        return DocumentId::from_bson(bson::Bson::ObjectId(oid));
    }
    // Fallback: treat as string ID
    DocumentId::from_bson(bson::Bson::String(id.to_string()))
}

/// Convert RangoError to napi Error.
fn rango_err_to_napi(err: RangoError) -> Error {
    Error::new(Status::GenericFailure, err.to_string())
}

#[napi]
pub struct RangoClient {
    client: rango_sdk::RangoClient<DegradingStorage<RedbStorage>>,
}

#[napi]
impl RangoClient {
    #[napi(constructor)]
    pub fn new(path: String, node_id: Option<String>) -> Result<Self> {
        let base_path = Path::new(&path);
        std::fs::create_dir_all(base_path).map_err(|e| {
            Error::new(
                Status::GenericFailure,
                format!("Failed to create workspace: {e}"),
            )
        })?;

        let storage_path = base_path.join("data.redb");
        let inner_storage = RedbStorage::open(&storage_path)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;
        let storage = Arc::new(
            DegradingStorage::with_default_threshold(inner_storage, &storage_path)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?,
        );

        let oplog_path = base_path.join("oplog.bin");
        let oplog = Arc::new(
            FileOplog::new(&oplog_path)
                .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?,
        );

        let node_id = node_id.unwrap_or_else(|| "node-js".to_string());
        let client = rango_sdk::RangoClient::open(storage, oplog, node_id)
            .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))?;

        Ok(Self { client })
    }

    #[napi]
    pub fn insert_one(&self, collection_name: String, json_doc: String) -> Result<String> {
        let document = json_str_to_document(&json_doc)?;
        let collection = self.client.collection(&collection_name);
        let id = collection.insert_one(document).map_err(rango_err_to_napi)?;
        Ok(id.to_string())
    }

    #[napi]
    pub fn find_one(&self, collection_name: String, id: String) -> Result<Option<String>> {
        let doc_id = parse_doc_id(&id);
        let collection = self.client.collection(&collection_name);
        let maybe_doc = collection.find_one(&doc_id).map_err(rango_err_to_napi)?;
        match maybe_doc {
            Some(doc) => Ok(Some(rango_doc_to_json(doc)?)),
            None => Ok(None),
        }
    }

    #[napi]
    pub fn find_many(&self, collection_name: String) -> Result<Vec<String>> {
        let collection = self.client.collection(&collection_name);
        let cursor = collection.find_many().map_err(rango_err_to_napi)?;
        let mut results = Vec::new();
        for result in cursor {
            let doc = result.map_err(rango_err_to_napi)?;
            results.push(rango_doc_to_json(doc)?);
        }
        Ok(results)
    }

    #[napi]
    pub fn update_one(
        &self,
        collection_name: String,
        id: String,
        json_update: String,
    ) -> Result<bool> {
        let doc_id = parse_doc_id(&id);
        let mut document = json_str_to_document(&json_update)?;
        // Auto-wrap in $set if no operators present (DX-friendly)
        let has_operators = document.keys().any(|k| k.starts_with('$'));
        if !has_operators {
            let mut wrapped = Document::new();
            wrapped.insert("$set", document);
            document = wrapped;
        }
        let collection = self.client.collection(&collection_name);
        collection
            .update_one(&doc_id, document)
            .map_err(rango_err_to_napi)
    }

    #[napi]
    pub fn delete_one(&self, collection_name: String, id: String) -> Result<bool> {
        let doc_id = parse_doc_id(&id);
        let collection = self.client.collection(&collection_name);
        collection.delete_one(&doc_id).map_err(rango_err_to_napi)
    }
}
