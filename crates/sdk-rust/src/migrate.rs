use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use bson::Document;
use rango_types::{CollectionName, RangoError};

use crate::RangoClient;

/// Import progress callback.
pub trait ImportProgress: Send + Sync {
    fn on_document(&self, count: usize);
    fn on_error(&self, line: usize, error: String);
    fn on_complete(&self, imported: usize, errors: usize);
}

/// Default no-op progress handler.
pub struct NoOpProgress;

impl ImportProgress for NoOpProgress {
    fn on_document(&self, _count: usize) {}
    fn on_error(&self, _line: usize, _error: String) {}
    fn on_complete(&self, _imported: usize, _errors: usize) {}
}

/// Simple console progress handler.
pub struct ConsoleProgress;

impl ImportProgress for ConsoleProgress {
    fn on_document(&self, count: usize) {
        if count % 100 == 0 {
            eprintln!("  Imported {} documents...", count);
        }
    }
    fn on_error(&self, line: usize, error: String) {
        eprintln!("  Error at line {}: {}", line, error);
    }
    fn on_complete(&self, imported: usize, errors: usize) {
        eprintln!("  Done: {} imported, {} errors", imported, errors);
    }
}

/// Import result.
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub imported: usize,
    pub errors: usize,
}

/// Export result.
#[derive(Debug, Clone)]
pub struct ExportResult {
    pub exported: usize,
}

impl RangoClient {
    /// Import documents from a JSON Lines file (one JSON document per line).
    /// Each line should be a valid JSON object. If the object has an `_id` field,
    /// it will be preserved (ObjectId strings are converted to BSON ObjectId).
    pub fn import_json<P: AsRef<Path>>(
        &self,
        collection: &str,
        path: P,
        progress: &dyn ImportProgress,
    ) -> Result<ImportResult, RangoError> {
        let file = File::open(path).map_err(|e| RangoError::Storage(e.to_string()))?;
        let reader = BufReader::new(file);
        let coll = CollectionName::new(collection);

        let mut imported = 0usize;
        let mut errors = 0usize;

        for (line_num, line) in reader.lines().enumerate() {
            let line = match line {
                Ok(l) => sanitize_json_line(line_num, &l),
                Err(e) => {
                    errors += 1;
                    progress.on_error(line_num + 1, e.to_string());
                    continue;
                }
            };

            if line.is_empty() {
                continue;
            }

            // Parse JSON to BSON Document
            let doc = match parse_mongo_json(&line) {
                Ok(d) => d,
                Err(e) => {
                    errors += 1;
                    progress.on_error(line_num + 1, e.to_string());
                    continue;
                }
            };

            match self.__engine().insert_one(&coll, doc) {
                Ok(_) => {
                    imported += 1;
                    progress.on_document(imported);
                }
                Err(e) => {
                    errors += 1;
                    progress.on_error(line_num + 1, e.to_string());
                }
            }
        }

        progress.on_complete(imported, errors);
        Ok(ImportResult { imported, errors })
    }

    /// Export all documents from a collection to a JSON Lines file.
    /// Documents are written as standard JSON (not extended JSON) with
    /// ObjectIds converted to their string representation for compatibility.
    pub fn export_json<P: AsRef<Path>>(
        &self,
        collection: &str,
        path: P,
    ) -> Result<ExportResult, RangoError> {
        let file = File::create(path).map_err(|e| RangoError::Storage(e.to_string()))?;
        let mut writer = BufWriter::new(file);
        let coll = CollectionName::new(collection);

        let cursor = self.__engine().find_many(&coll)?;
        let mut exported = 0usize;

        for result in cursor {
            let doc = result?;
            let json_str = match doc_to_json(&doc.data) {
                Ok(s) => s,
                Err(_e) => {
                    // Skip documents that can't be serialized
                    continue;
                }
            };

            writeln!(writer, "{}", json_str).map_err(|e| RangoError::Storage(e.to_string()))?;
            exported += 1;
        }

        writer
            .flush()
            .map_err(|e| RangoError::Storage(e.to_string()))?;
        Ok(ExportResult { exported })
    }
}

/// Parse a MongoDB extended JSON string into a BSON Document.
/// Handles common extended JSON formats:
/// - `{ "$oid": "..." }` -> ObjectId
/// - `{ "$date": "..." }` -> DateTime
/// - `{ "$numberInt": "..." }` -> Int32
/// - `{ "$numberLong": "..." }` -> Int64
/// - `{ "$numberDouble": "..." }` -> Double
fn parse_mongo_json(line: &str) -> Result<Document, RangoError> {
    // First, try to parse as standard JSON
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|e| RangoError::Storage(format!("JSON parse error: {}", e)))?;

    // Convert JSON Value to BSON Document, handling extended JSON
    let bson_value = json_to_bson(value)?;

    match bson_value {
        bson::Bson::Document(doc) => Ok(doc),
        _ => Err(RangoError::Storage(
            "Expected JSON object, got other type".to_string(),
        )),
    }
}

fn sanitize_json_line(line_num: usize, raw: &str) -> String {
    let trimmed = raw.trim();
    if line_num == 0 {
        trimmed.trim_start_matches('\u{feff}').to_string()
    } else {
        trimmed.to_string()
    }
}

/// Convert serde_json::Value to bson::Bson, handling MongoDB extended JSON.
fn json_to_bson(value: serde_json::Value) -> Result<bson::Bson, RangoError> {
    match value {
        serde_json::Value::Null => Ok(bson::Bson::Null),
        serde_json::Value::Bool(b) => Ok(bson::Bson::Boolean(b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    Ok(bson::Bson::Int32(i as i32))
                } else {
                    Ok(bson::Bson::Int64(i))
                }
            } else if let Some(f) = n.as_f64() {
                Ok(bson::Bson::Double(f))
            } else {
                Err(RangoError::Storage("Invalid number".to_string()))
            }
        }
        serde_json::Value::String(s) => Ok(bson::Bson::String(s)),
        serde_json::Value::Array(arr) => {
            let bson_arr: Result<Vec<_>, _> = arr.into_iter().map(json_to_bson).collect();
            Ok(bson::Bson::Array(bson_arr?))
        }
        serde_json::Value::Object(map) => {
            // Check for extended JSON patterns
            if map.len() == 1 {
                if let Some((key, val)) = map.iter().next() {
                    match key.as_str() {
                        "$oid" => {
                            if let serde_json::Value::String(oid_str) = val {
                                let oid = bson::oid::ObjectId::parse_str(oid_str).map_err(|e| {
                                    RangoError::Storage(format!("Invalid ObjectId: {}", e))
                                })?;
                                return Ok(bson::Bson::ObjectId(oid));
                            }
                        }
                        "$date" => {
                            if let serde_json::Value::String(date_str) = val {
                                let dt =
                                    bson::DateTime::parse_rfc3339_str(date_str).map_err(|e| {
                                        RangoError::Storage(format!("Invalid date: {}", e))
                                    })?;
                                return Ok(bson::Bson::DateTime(dt));
                            }
                        }
                        "$numberInt" => {
                            if let serde_json::Value::String(s) = val {
                                let i: i32 = s.parse().map_err(|e| {
                                    RangoError::Storage(format!("Invalid Int32: {}", e))
                                })?;
                                return Ok(bson::Bson::Int32(i));
                            }
                        }
                        "$numberLong" => {
                            if let serde_json::Value::String(s) = val {
                                let i: i64 = s.parse().map_err(|e| {
                                    RangoError::Storage(format!("Invalid Int64: {}", e))
                                })?;
                                return Ok(bson::Bson::Int64(i));
                            }
                        }
                        "$numberDouble" => {
                            let f: f64 = match val {
                                serde_json::Value::String(s) => s.parse().map_err(|e| {
                                    RangoError::Storage(format!("Invalid Double: {}", e))
                                })?,
                                serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
                                _ => 0.0,
                            };
                            return Ok(bson::Bson::Double(f));
                        }
                        _ => {}
                    }
                }
            }

            // Regular object
            let mut doc = Document::new();
            for (k, v) in map {
                doc.insert(k, json_to_bson(v)?);
            }
            Ok(bson::Bson::Document(doc))
        }
    }
}

/// Convert a BSON Document to a JSON string.
/// ObjectIds are converted to strings for standard JSON compatibility.
fn doc_to_json(doc: &Document) -> Result<String, RangoError> {
    let json_value = bson_to_json(bson::Bson::Document(doc.clone()))?;
    serde_json::to_string(&json_value)
        .map_err(|e| RangoError::Storage(format!("JSON serialize error: {}", e)))
}

/// Convert bson::Bson to serde_json::Value.
fn bson_to_json(value: bson::Bson) -> Result<serde_json::Value, RangoError> {
    match value {
        bson::Bson::Double(d) => Ok(serde_json::Value::Number(
            serde_json::Number::from_f64(d).unwrap_or_else(|| serde_json::Number::from(0)),
        )),
        bson::Bson::String(s) => Ok(serde_json::Value::String(s)),
        bson::Bson::Array(arr) => {
            let json_arr: Result<Vec<_>, _> = arr.into_iter().map(bson_to_json).collect();
            Ok(serde_json::Value::Array(json_arr?))
        }
        bson::Bson::Document(doc) => {
            let mut map = serde_json::Map::new();
            for (k, v) in doc {
                map.insert(k, bson_to_json(v)?);
            }
            Ok(serde_json::Value::Object(map))
        }
        bson::Bson::Boolean(b) => Ok(serde_json::Value::Bool(b)),
        bson::Bson::Null => Ok(serde_json::Value::Null),
        bson::Bson::Int32(i) => Ok(serde_json::Value::Number(i.into())),
        bson::Bson::Int64(i) => Ok(serde_json::Value::Number(i.into())),
        bson::Bson::ObjectId(oid) => Ok(serde_json::Value::String(oid.to_string())),
        bson::Bson::DateTime(dt) => Ok(serde_json::Value::String(dt.to_string())),
        bson::Bson::Binary(bin) => {
            // For UUIDs, output as string
            if bin.subtype == bson::spec::BinarySubtype::Uuid {
                if let Ok(uuid) = uuid::Uuid::from_slice(&bin.bytes) {
                    return Ok(serde_json::Value::String(uuid.to_string()));
                }
            }
            // Otherwise hex string
            Ok(serde_json::Value::String(format!(
                "<Binary {:02x?}>",
                bin.subtype
            )))
        }
        bson::Bson::Undefined => Ok(serde_json::Value::Null),
        bson::Bson::RegularExpression(regex) => Ok(serde_json::Value::String(format!(
            "/{}/{}",
            regex.pattern, regex.options
        ))),
        bson::Bson::JavaScriptCode(code) => Ok(serde_json::Value::String(code)),
        bson::Bson::JavaScriptCodeWithScope(_) => Ok(serde_json::Value::String(
            "[JS code with scope]".to_string(),
        )),
        bson::Bson::Timestamp(ts) => Ok(serde_json::Value::Number(ts.time.into())),
        bson::Bson::Decimal128(d) => Ok(serde_json::Value::String(d.to_string())),
        bson::Bson::MaxKey => Ok(serde_json::Value::String("$MaxKey".to_string())),
        bson::Bson::MinKey => Ok(serde_json::Value::String("$MinKey".to_string())),
        bson::Bson::DbPointer(_) => Ok(serde_json::Value::Null),
        bson::Bson::Symbol(s) => Ok(serde_json::Value::String(s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    fn create_test_client() -> RangoClient {
        let storage = Arc::new(rango_storage::MemoryStorage::new());
        let oplog = Arc::new(rango_oplog::NullOplog::new());
        RangoClient::open(storage, oplog, "test-node").unwrap()
    }

    #[test]
    fn test_import_json_lines() {
        let client = create_test_client();

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"name": "Alice", "age": 30}}"#).unwrap();
        writeln!(file, r#"{{"name": "Bob", "age": 25}}"#).unwrap();
        writeln!(file, r#"{{"name": "Charlie", "age": 35}}"#).unwrap();
        file.flush().unwrap();

        let result = client
            .import_json("people", file.path(), &NoOpProgress)
            .unwrap();
        assert_eq!(result.imported, 3);
        assert_eq!(result.errors, 0);

        let coll = client.collection("people");
        let cursor = coll.find_many().unwrap();
        let docs: Vec<_> = cursor.filter_map(|r| r.ok()).collect();
        assert_eq!(docs.len(), 3);
    }

    #[test]
    fn test_import_preserves_objectid() {
        let client = create_test_client();

        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"{{"_id": {{"$oid": "507f1f77bcf86cd799439011"}}, "name": "Alice"}}"#
        )
        .unwrap();
        file.flush().unwrap();

        let result = client
            .import_json("people", file.path(), &NoOpProgress)
            .unwrap();
        assert_eq!(result.imported, 1);

        let coll = client.collection("people");
        let cursor = coll.find_many().unwrap();
        let docs: Vec<_> = cursor.filter_map(|r| r.ok()).collect();
        assert_eq!(docs.len(), 1);

        // Verify ObjectId was preserved
        match &docs[0].data.get("_id").unwrap() {
            bson::Bson::ObjectId(oid) => {
                assert_eq!(oid.to_string(), "507f1f77bcf86cd799439011");
            }
            other => panic!("Expected ObjectId, got {:?}", other),
        }
    }

    #[test]
    fn test_import_skips_invalid_lines() {
        let client = create_test_client();

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"name": "Alice"}}"#).unwrap();
        writeln!(file, r#"this is not json"#).unwrap();
        writeln!(file, r#"{{"name": "Bob"}}"#).unwrap();
        file.flush().unwrap();

        let result = client
            .import_json("people", file.path(), &NoOpProgress)
            .unwrap();
        assert_eq!(result.imported, 2);
        assert_eq!(result.errors, 1);
    }

    #[test]
    fn test_export_json() {
        let client = create_test_client();

        // Insert some docs
        let coll = client.collection("people");
        coll.insert_one(bson::doc! { "name": "Alice", "age": 30 })
            .unwrap();
        coll.insert_one(bson::doc! { "name": "Bob", "age": 25 })
            .unwrap();

        let output = NamedTempFile::new().unwrap();
        let result = client.export_json("people", output.path()).unwrap();
        assert_eq!(result.exported, 2);

        // Verify output is valid JSON lines
        let reader = BufReader::new(File::open(output.path()).unwrap());
        let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON
        for line in &lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(parsed.get("name").is_some());
        }
    }

    #[test]
    fn test_import_extended_json_types() {
        let client = create_test_client();

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, r#"{{"int": {{"$numberInt": "42"}}, "long": {{"$numberLong": "9007199254740992"}}, "double": {{"$numberDouble": "3.14"}}, "date": {{"$date": "2024-01-15T10:30:00Z"}}}}"#).unwrap();
        file.flush().unwrap();

        let result = client
            .import_json("types", file.path(), &NoOpProgress)
            .unwrap();
        assert_eq!(result.imported, 1);

        let coll = client.collection("types");
        let cursor = coll.find_many().unwrap();
        let docs: Vec<_> = cursor.filter_map(|r| r.ok()).collect();
        assert_eq!(docs.len(), 1);

        let doc = &docs[0].data;
        assert_eq!(doc.get_i32("int").unwrap(), 42);
        assert_eq!(doc.get_i64("long").unwrap(), 9007199254740992i64);
        let expected_double = 314_f64 / 100.0;
        assert!((doc.get_f64("double").unwrap() - expected_double).abs() < 0.001);
    }

    #[test]
    fn test_import_json_with_utf8_bom_first_line() {
        let client = create_test_client();

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "\u{feff}{{\"name\": \"Alice\"}}").unwrap();
        writeln!(file, r#"{{"name": "Bob"}}"#).unwrap();
        file.flush().unwrap();

        let result = client
            .import_json("people", file.path(), &NoOpProgress)
            .unwrap();
        assert_eq!(result.imported, 2);
        assert_eq!(result.errors, 0);
    }
}
