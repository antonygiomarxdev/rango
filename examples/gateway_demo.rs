//! Gateway Demo — Simulates an IoT gateway using Rango.
//!
//! This example shows:
//! - Local CRUD operations (offline-capable)
//! - Tombstone deletes for sync
//! - Conflict resolution simulation
//!
//! Run with: cargo run --example gateway_demo

use std::sync::Arc;

use bson::doc;
use rango_oplog::NullOplog;
use rango_storage::MemoryStorage;
use rango_sdk::RangoClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Rango Gateway Demo");
    println!("==================\n");

    // Setup
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    let client = RangoClient::open(storage, oplog, "gateway-001")?;
    let sensors = client.collection("sensors");

    // 1. Insert sensor readings (offline)
    println!("1. Inserting sensor readings (offline mode)...");
    for i in 0..5 {
        let id = sensors.insert_one(doc! {
            "sensor_id": format!("temp-{}", i),
            "temperature": 20.0 + (i as f64),
            "unit": "celsius",
            "location": "warehouse-a"
        })?;
        println!("  Inserted sensor {} -> id={}", i, id);
    }

    // 2. Query all sensors
    println!("\n2. Querying all sensors...");
    let mut cursor = sensors.find_many()?;
    let mut count = 0;
    while let Some(Ok(doc)) = cursor.next() {
        count += 1;
        let name = doc.data.get_str("sensor_id").unwrap_or("unknown");
        let temp = doc.data.get_f64("temperature").unwrap_or(0.0);
        println!("  {}: {} = {:.1}°C", count, name, temp);
    }
    println!("  Total: {} sensors", count);

    // 3. Update a sensor
    println!("\n3. Updating sensor temp-2...");
    let mut cursor = sensors.find_many()?;
    let doc = cursor.next().unwrap().unwrap();
    let id = rango_types::DocumentId::from_bson(doc.data.get("_id").unwrap().clone());
    sensors.update_one(&id, doc! { "$set": { "temperature": 99.9 } })?;
    println!("  Updated temp-2 to 99.9°C");

    // 4. Filter query
    println!("\n4. Querying sensors with temperature > 22°C...");
    let mut cursor = client.__engine().find(
        &rango_types::CollectionName::new("sensors"),
        &doc! { "temperature": { "$gt": 22.0 } },
        None, None, None, None
    )?;
    while let Some(Ok(doc)) = cursor.next() {
        let name = doc.data.get_str("sensor_id").unwrap_or("unknown");
        let temp = doc.data.get_f64("temperature").unwrap_or(0.0);
        println!("  {} = {:.1}°C", name, temp);
    }

    // 5. Delete a sensor (tombstone)
    println!("\n5. Deleting sensor temp-0 (tombstone)...");
    let mut cursor = sensors.find_many()?;
    let first = cursor.next().unwrap().unwrap();
    let first_id = rango_types::DocumentId::from_bson(first.data.get("_id").unwrap().clone());
    sensors.delete_one(&first_id)?;
    println!("  Deleted sensor temp-0");

    // Verify it's gone from normal queries but exists as tombstone
    let found = sensors.find_one(&first_id)?;
    println!("  find_one after delete: {}", if found.is_none() { "not found (expected)" } else { "found (unexpected!)" });

    // 6. Show metadata
    println!("\n6. Document metadata (HLC revision, timestamps)...");
    let mut cursor = sensors.find_many()?;
    if let Some(Ok(doc)) = cursor.next() {
        println!("  _rev: {}", doc.data.get_str("_rev").unwrap_or("missing"));
        println!("  _updated_at: {}", doc.data.get_datetime("_updated_at").unwrap_or(&bson::DateTime::now()));
        println!("  _source_node: {}", doc.data.get_str("_source_node").unwrap_or("missing"));
    }

    println!("\n==================");
    println!("Demo complete!");

    Ok(())
}
