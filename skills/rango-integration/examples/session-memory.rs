// Session Memory Example — Rust
// Demonstrates persisting user sessions with governance metadata

use std::sync::Arc;
use bson::{doc, Document};
use rango_sdk::RangoClient;
use rango_storage::{DegradingStorage, RedbStorage};
use rango_oplog::FileOplog;
use rango_types::*;
use uuid::Uuid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open workspace
    let path = std::path::Path::new("./session-memory-example.rango");
    std::fs::create_dir_all(path)?;
    
    let storage_path = path.join("data.redb");
    let inner = RedbStorage::open(&storage_path)?;
    let storage = Arc::new(
        DegradingStorage::with_default_threshold(inner, &storage_path)?
    );
    
    let oplog = Arc::new(FileOplog::new(path.join("oplog.bin"))?);
    let client = RangoClient::open(storage, oplog, "session-example")?;
    
    // Insert session with governance metadata
    let session_id = Uuid::new_v7().to_string();
    let session_doc = doc! {
        "user_id": "alice",
        "session_token": session_id.clone(),
        "created_at": bson::DateTime::now(),
        "tenant_id": "org-123",          // Governance: tenant scope
        "lineage": format!("session:{}", session_id), // Governance: provenance
        "trust_score": 1.0,              // Governance: verified internal
        "verified": true,
        "expires_at": bson::DateTime::now().timestamp_millis() + 3600000i64,
    };
    
    let sessions = client.collection("sessions");
    let doc_id = sessions.insert_one(session_doc)?;
    
    println!("Session created: {}", doc_id);
    
    // Retrieve session
    if let Some(doc) = sessions.find_one(&doc_id)? {
        println!("Found session: {:?}", doc.data);
    }
    
    Ok(())
}
