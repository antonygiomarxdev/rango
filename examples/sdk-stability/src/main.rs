/// SDK Stability Compile-Time Integration Example
///
/// This example exercises the entire stable API surface of rango-sdk as of v0.1.0.
/// **If this file fails to compile after a SDK change, the change is breaking and must
/// follow `docs/reference/sdk-stability.md`.**
///
/// This example is used in CI to gate breaking changes. External products relying on
/// the Rango SDK should be able to compile and run this code across minor versions.
use bson::doc;
use rango_oplog::FileOplog;
use rango_sdk::{
    ConsoleProgress, Document, NoOpProgress, RangoClient, RankingSignals, RetrievalCandidate,
    RetrievalCapabilityRequest, RetrievalCapabilityResponse, RetrievalSource, RetrievalStatus,
};
use rango_storage::RedbStorage;
use rango_types::RangoError;
use std::sync::Arc;
use tempfile::TempDir;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary workspace
    let temp = TempDir::new()?;
    let workspace_path = temp.path();

    // Initialize storage and oplog
    let storage = Arc::new(RedbStorage::open(workspace_path.join("data.redb"))?);
    let oplog = Arc::new(FileOplog::new(workspace_path.join("oplog.rgo"))?);

    // Test: RangoClient::open (stable)
    let client = RangoClient::open(storage, oplog, "stability-test-node")?;
    println!("✓ RangoClient::open works");

    // Test: RangoClient::collection (stable)
    let collection = client.collection("documents");
    println!("✓ RangoClient::collection works");

    // Test: CollectionClient::insert_one (stable)
    let doc1 = doc! {
        "name": "Alice",
        "score": 95,
        "verified": true,
    };
    let id1 = collection.insert_one(doc1)?;
    println!("✓ CollectionClient::insert_one works");

    // Test: CollectionClient::find_one (stable)
    let found = collection.find_one(&id1)?;
    assert!(found.is_some(), "Document should be found");
    println!("✓ CollectionClient::find_one works");

    // Test: CollectionClient::update_one (stable)
    let updated = collection.update_one(&id1, doc! { "$set": { "score": 98 } })?;
    assert!(updated, "Document should be updated");
    println!("✓ CollectionClient::update_one works");

    // Insert more documents for find_many test
    let doc2 = doc! { "name": "Bob", "score": 87 };
    let doc3 = doc! { "name": "Charlie", "score": 92 };
    collection.insert_one(doc2)?;
    collection.insert_one(doc3)?;

    // Test: CollectionClient::find_many (stable) -> returns Cursor (stable)
    let cursor = collection.find_many()?;
    let count = cursor.filter_map(|r| r.ok()).count();
    assert_eq!(count, 3, "Should find 3 documents");
    println!("✓ CollectionClient::find_many works");

    // Test: Cursor is iterable (stable)
    let cursor = collection.find_many()?;
    let mut doc_count = 0;
    for result in cursor {
        let _doc = result?;
        doc_count += 1;
    }
    assert_eq!(doc_count, 3);
    println!("✓ Cursor iteration works");

    // Test: CollectionClient::delete_one (stable)
    let deleted = collection.delete_one(&id1)?;
    assert!(deleted, "Document should be deleted");
    println!("✓ CollectionClient::delete_one works");

    // Test: RangoClient::export_json (stable)
    let export_path = temp.path().join("export.json");
    let export_result = client.export_json("documents", &export_path)?;
    assert!(export_result.exported > 0);
    println!("✓ RangoClient::export_json works");

    // Test: RangoClient::import_json (stable)
    // Create a test import file
    let import_path = temp.path().join("import.jsonl");
    std::fs::write(
        &import_path,
        r#"{"name":"Diana","score":88}
{"name":"Eve","score":91}"#,
    )?;

    let no_op_progress = NoOpProgress;
    let import_result = client.import_json("imported", &import_path, &no_op_progress)?;
    assert_eq!(import_result.imported, 2);
    assert_eq!(import_result.errors, 0);
    println!("✓ RangoClient::import_json with NoOpProgress works");

    // Test: ImportProgress trait with ConsoleProgress (stable)
    let import_path2 = temp.path().join("import2.jsonl");
    std::fs::write(&import_path2, r#"{"name":"Frank","score":89}"#)?;

    let console_progress = ConsoleProgress;
    let import_result2 = client.import_json("imported2", &import_path2, &console_progress)?;
    assert_eq!(import_result2.imported, 1);
    println!("✓ RangoClient::import_json with ConsoleProgress works");

    // Test: Re-exported Document type (stable)
    let _test_doc: Document = doc! { "test": "value" };
    println!("✓ rango_sdk::Document (re-export) works");

    // Test: Re-exported types from rango_types (stable)
    let _candidate: RetrievalCandidate = RetrievalCandidate {
        candidate_id: "test-id".to_string(),
        tenant_id: "test-tenant".to_string(),
        namespace: "test-ns".to_string(),
        source: RetrievalSource::Canonical,
        lineage: "test".to_string(),
        timestamp_ms: 1000,
        payload: doc! {},
        signals: RankingSignals {
            relevance: 0.95,
            recency: 0.8,
            trust: 0.9,
            provenance: 0.85,
        },
        score: 0.88,
        explainability: None,
    };
    println!("✓ RetrievalCandidate (re-export) works");

    let _request = RetrievalCapabilityRequest {
        tenant_id: "test-tenant".to_string(),
        namespace: "test-ns".to_string(),
        query: "test query".to_string(),
        limit: 10,
        vector_limit: 5,
        graph_limit: 3,
    };
    println!("✓ RetrievalCapabilityRequest (re-export) works");

    let _response = RetrievalCapabilityResponse {
        status: RetrievalStatus::Healthy,
        retrieval_status_reason: "test".to_string(),
        canonical_fallback: false,
        candidates: vec![],
    };
    println!("✓ RetrievalCapabilityResponse (re-export) works");

    let _status = RetrievalStatus::Healthy;
    println!("✓ RetrievalStatus (re-export) works");

    // Verify RangoError is accessible (if exposed in public API)
    let _err: Result<(), RangoError> = Ok(());
    println!("✓ RangoError (type signature) works");

    println!("\n✅ All stable SDK surface items compile and function correctly.");
    println!("   This workspace is safe to update across v0.1.x releases.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stable_api_compiles() {
        // This test just verifies that the code compiles.
        // Running main() is the real validation.
    }
}
