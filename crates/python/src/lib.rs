use std::path::Path;
use std::sync::Arc;

use bson::{Bson, Document};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use rango_oplog::FileOplog;
use rango_storage::{DegradingStorage, RedbStorage};
use rango_types::{DocumentId, RangoDocument, RangoError};
/// Convert a Python dict to a bson::Document via JSON bridge.
fn py_dict_to_document(dict: &Bound<'_, PyDict>) -> PyResult<Document> {
    let json_module = dict.py().import("json")?;
    let json_str = json_module
        .call_method1("dumps", (dict,))?
        .extract::<String>()?;
    let json_value: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| PyRuntimeError::new_err(format!("JSON parse error: {e}")))?;
    let doc: Document = serde_json::from_value(json_value)
        .map_err(|e| PyRuntimeError::new_err(format!("BSON conversion error: {e}")))?;
    Ok(doc)
}

/// Convert a bson::Document to a Python dict via JSON bridge.
fn document_to_py_dict<'py>(py: Python<'py>, doc: &Document) -> PyResult<Bound<'py, PyDict>> {
    let json_value = serde_json::to_value(doc)
        .map_err(|e| PyRuntimeError::new_err(format!("JSON serialization error: {e}")))?;
    let json_str = serde_json::to_string(&json_value)
        .map_err(|e| PyRuntimeError::new_err(format!("JSON stringify error: {e}")))?;
    let json_module = py.import("json")?;
    let py_obj = json_module.call_method1("loads", (json_str,))?;
    py_obj
        .downcast_into::<PyDict>()
        .map_err(|_| PyRuntimeError::new_err("Expected dict from JSON deserialization"))
}

/// Convert RangoDocument to Python dict with _id and _rev fields.
fn rango_doc_to_py_dict<'py>(py: Python<'py>, doc: RangoDocument) -> PyResult<Bound<'py, PyDict>> {
    let dict = document_to_py_dict(py, &doc.data)?;
    if let Some(id) = doc.id() {
        dict.set_item("_id", id.to_string())?;
    }
    if let Some(rev) = doc.revision() {
        dict.set_item("_rev", rev)?;
    }
    Ok(dict)
}

/// Convert RangoError to PyRuntimeError.
fn rango_err_to_py(err: RangoError) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

/// Parse a document ID string into a DocumentId.
/// Supports UUID (v7 and others), MongoDB ObjectId, and arbitrary strings.
fn parse_doc_id(id: &str) -> DocumentId {
    // Try UUID first
    if let Ok(uuid) = uuid::Uuid::parse_str(id) {
        return DocumentId::from_bson(Bson::Binary(bson::Binary {
            subtype: bson::spec::BinarySubtype::Uuid,
            bytes: uuid.as_bytes().to_vec(),
        }));
    }
    // Try ObjectId
    if let Ok(oid) = bson::oid::ObjectId::parse_str(id) {
        return DocumentId::from_bson(Bson::ObjectId(oid));
    }
    // Fallback: treat as string ID
    DocumentId::from_bson(Bson::String(id.to_string()))
}

/// A Python-facing Rango client backed by RedbStorage and FileOplog.
///
/// Usage:
/// ```python
/// import rango
/// client = rango.RangoClient("/tmp/workspace")
/// doc_id = client.insert_one("memories", {"content": "hello"})
/// doc = client.find_one("memories", doc_id)
/// ```
#[pyclass(name = "RangoClient")]
pub struct PyRangoClient {
    client: rango_sdk::RangoClient<DegradingStorage<RedbStorage>>,
}

#[pymethods]
impl PyRangoClient {
    /// Open a Rango workspace at the given path.
    ///
    /// Creates a Redb database at `<path>/data.redb` and a FileOplog at `<path>/oplog.bin`.
    #[new]
    #[pyo3(signature = (path, node_id = "python-node"))]
    fn new(path: &str, node_id: &str) -> PyResult<Self> {
        let base_path = Path::new(path);
        std::fs::create_dir_all(base_path)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create workspace: {e}")))?;

        let storage_path = base_path.join("data.redb");
        let inner_storage = RedbStorage::open(&storage_path).map_err(rango_err_to_py)?;
        let storage = Arc::new(
            DegradingStorage::with_default_threshold(inner_storage, &storage_path)
                .map_err(rango_err_to_py)?,
        );

        let oplog_path = base_path.join("oplog.bin");
        let oplog = Arc::new(FileOplog::new(&oplog_path).map_err(rango_err_to_py)?);

        let client =
            rango_sdk::RangoClient::open(storage, oplog, node_id).map_err(rango_err_to_py)?;

        Ok(Self { client })
    }

    /// Insert a document into a collection.
    ///
    /// Args:
    ///     collection_name: Name of the collection
    ///     doc: Document as a Python dict
    ///
    /// Returns:
    ///     Document ID as a string
    #[pyo3(signature = (collection_name, doc))]
    fn insert_one(&self, collection_name: &str, doc: &Bound<'_, PyDict>) -> PyResult<String> {
        let document = py_dict_to_document(doc)?;
        let collection = self.client.collection(collection_name);
        let id = collection.insert_one(document).map_err(rango_err_to_py)?;
        Ok(id.to_string())
    }

    /// Find a document by ID.
    ///
    /// Args:
    ///     collection_name: Name of the collection
    ///     id: Document ID string
    ///
    /// Returns:
    ///     Document as dict, or None if not found
    #[pyo3(signature = (collection_name, id))]
    fn find_one<'py>(
        &self,
        py: Python<'py>,
        collection_name: &str,
        id: &str,
    ) -> PyResult<Option<Bound<'py, PyDict>>> {
        let doc_id = parse_doc_id(id);
        let collection = self.client.collection(collection_name);
        let maybe_doc = collection.find_one(&doc_id).map_err(rango_err_to_py)?;
        match maybe_doc {
            Some(doc) => Ok(Some(rango_doc_to_py_dict(py, doc)?)),
            None => Ok(None),
        }
    }

    /// Find all documents in a collection.
    ///
    /// Returns:
    ///     List of documents as dicts
    #[pyo3(signature = (collection_name))]
    fn find_many<'py>(
        &self,
        py: Python<'py>,
        collection_name: &str,
    ) -> PyResult<Vec<Bound<'py, PyDict>>> {
        let collection = self.client.collection(collection_name);
        let cursor = collection.find_many().map_err(rango_err_to_py)?;
        let mut results = Vec::new();
        for result in cursor {
            let doc = result.map_err(rango_err_to_py)?;
            results.push(rango_doc_to_py_dict(py, doc)?);
        }
        Ok(results)
    }

    /// Update a document by ID.
    ///
    /// Args:
    ///     collection_name: Name of the collection
    ///     id: Document ID string
    ///     update: Update document as Python dict
    ///
    /// Returns:
    ///     True if document was found and updated
    #[pyo3(signature = (collection_name, id, update))]
    fn update_one(
        &self,
        collection_name: &str,
        id: &str,
        update: &Bound<'_, PyDict>,
    ) -> PyResult<bool> {
        let doc_id = parse_doc_id(id);
        let mut document = py_dict_to_document(update)?;
        // If update doesn't contain MongoDB operators, wrap in $set for DX
        let has_operators = document.keys().any(|k| k.starts_with('$'));
        if !has_operators {
            let mut wrapped = Document::new();
            wrapped.insert("$set", document);
            document = wrapped;
        }
        let collection = self.client.collection(collection_name);
        collection
            .update_one(&doc_id, document)
            .map_err(rango_err_to_py)
    }

    /// Delete a document by ID.
    ///
    /// Args:
    ///     collection_name: Name of the collection
    ///     id: Document ID string
    ///
    /// Returns:
    ///     True if document was found and deleted
    #[pyo3(signature = (collection_name, id))]
    fn delete_one(&self, collection_name: &str, id: &str) -> PyResult<bool> {
        let doc_id = parse_doc_id(id);
        let collection = self.client.collection(collection_name);
        collection.delete_one(&doc_id).map_err(rango_err_to_py)
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRangoClient>()?;
    Ok(())
}
