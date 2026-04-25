//! OpenClaw integration smoke. Validates the Rango SDK against
//! `docs/integrations/openclaw-memory-contract.json`. If this binary fails to compile or run,
//! OpenClaw integration is broken or the memory contract drifted.

use anyhow::{Context, Result, bail, ensure};
use bson::{Bson, doc};
use rango_oplog::FileOplog;
use rango_sdk::RangoClient;
use rango_storage::RedbStorage;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

fn main() -> Result<()> {
    println!("🔍 OpenClaw integration smoke test starting...\n");

    // Step 1: Locate and load the contract JSON
    let contract_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/integrations/openclaw-memory-contract.json");
    println!("📄 Contract path: {}", contract_path.display());

    let contract_bytes = std::fs::read(&contract_path)
        .with_context(|| format!("Failed to read contract from {}", contract_path.display()))?;

    // Step 2: Compute SHA-256 of contract bytes
    let mut hasher = Sha256::new();
    hasher.update(&contract_bytes);
    let computed_hash = format!("{:x}", hasher.finalize());
    println!("   Computed contract hash: {}", computed_hash);

    // Step 3: Read expected hash from fixture
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("contract.sha256");
    let expected_hash = std::fs::read_to_string(&fixture_path)
        .with_context(|| format!("Failed to read fixture from {}", fixture_path.display()))?
        .trim()
        .to_string();
    println!("   Expected contract hash: {}", expected_hash);

    // Verify contract hash matches
    if computed_hash != expected_hash {
        bail!(
            "Contract hash mismatch!\n  Expected: {}\n  Computed: {}\n  Path: {}\n  \
             Action: contract drifted — review and update fixture if intentional.\n  \
             Use: sha256sum {} | cut -d' ' -f1 > {}",
            expected_hash,
            computed_hash,
            contract_path.display(),
            contract_path.display(),
            fixture_path.display()
        );
    }
    println!("✅ Contract hash verified\n");

    // Step 4: Create a tempdir workspace
    let temp = TempDir::new().context("Failed to create temp directory")?;
    let workspace_path = temp.path();
    println!("📂 Workspace: {}", workspace_path.display());

    // Step 5: Open RangoClient
    let storage = Arc::new(
        RedbStorage::open(workspace_path.join("data.redb"))
            .context("Failed to open RedbStorage")?,
    );
    let oplog = Arc::new(
        FileOplog::new(workspace_path.join("oplog.rgo")).context("Failed to create FileOplog")?,
    );

    let client = RangoClient::open(storage, oplog, "openclaw-smoke")
        .context("Failed to open RangoClient")?;
    println!("✅ RangoClient opened\n");

    // Step 6: Exercise write_path - insert one record into each contract collection
    println!("📝 Write path: inserting records into each collection...");

    let agent_state_doc = doc! {
        "tenant_id": "default",
        "namespace": "openclaw",
        "agent_id": "test-agent-1",
        "status": "idle",
        "active_task_id": Bson::Null,
        "updated_at": "2026-04-24T00:00:00Z"
    };
    let agent_state_id = client
        .collection("agent_state")
        .insert_one(agent_state_doc.clone())
        .context("Failed to insert into agent_state")?;
    println!("  ✓ agent_state: {}", agent_state_id);

    let task_state_doc = doc! {
        "tenant_id": "default",
        "namespace": "openclaw",
        "task_id": "test-task-1",
        "agent_id": "test-agent-1",
        "state": "running",
        "summary": "Test task",
        "updated_at": "2026-04-24T00:00:00Z"
    };
    let task_state_id = client
        .collection("task_state")
        .insert_one(task_state_doc.clone())
        .context("Failed to insert into task_state")?;
    println!("  ✓ task_state: {}", task_state_id);

    let episodes_doc = doc! {
        "tenant_id": "default",
        "namespace": "openclaw",
        "event_type": "tool_call",
        "agent_id": "test-agent-1",
        "task_id": "test-task-1",
        "payload": {"tool": "test_tool"},
        "created_at": "2026-04-24T00:00:00Z",
        "trust_score": 0.95
    };
    let episodes_id = client
        .collection("episodes")
        .insert_one(episodes_doc.clone())
        .context("Failed to insert into episodes")?;
    println!("  ✓ episodes: {}", episodes_id);

    let facts_doc = doc! {
        "tenant_id": "default",
        "namespace": "openclaw",
        "entity_key": "test-entity",
        "fact": "Test fact",
        "confidence": 0.85,
        "source_event_ids": [],
        "verified": false,
        "updated_at": "2026-04-24T00:00:00Z"
    };
    let facts_id = client
        .collection("facts")
        .insert_one(facts_doc.clone())
        .context("Failed to insert into facts")?;
    println!("  ✓ facts: {}", facts_id);
    println!("✅ Write path complete\n");

    // Step 7: Exercise read_path - find_one each inserted record
    println!("🔎 Read path: retrieving records...");

    let agent_state_retrieved = client
        .collection("agent_state")
        .find_one(&agent_state_id)
        .context("Failed to find agent_state")?;
    ensure!(
        agent_state_retrieved.is_some(),
        "agent_state record not found by id"
    );
    let agent_state_doc_found = agent_state_retrieved.unwrap();
    ensure!(
        agent_state_doc_found.data.get("tenant_id") == Some(&Bson::String("default".to_string())),
        "agent_state missing or invalid tenant_id"
    );
    ensure!(
        agent_state_doc_found.data.get("namespace") == Some(&Bson::String("openclaw".to_string())),
        "agent_state missing or invalid namespace"
    );
    println!("  ✓ agent_state: {}", agent_state_id);

    let task_state_retrieved = client
        .collection("task_state")
        .find_one(&task_state_id)
        .context("Failed to find task_state")?;
    ensure!(
        task_state_retrieved.is_some(),
        "task_state record not found by id"
    );
    let task_state_doc_found = task_state_retrieved.unwrap();
    ensure!(
        task_state_doc_found.data.get("tenant_id") == Some(&Bson::String("default".to_string())),
        "task_state missing or invalid tenant_id"
    );
    println!("  ✓ task_state: {}", task_state_id);

    let episodes_retrieved = client
        .collection("episodes")
        .find_one(&episodes_id)
        .context("Failed to find episodes")?;
    ensure!(
        episodes_retrieved.is_some(),
        "episodes record not found by id"
    );
    println!("  ✓ episodes: {}", episodes_id);

    let facts_retrieved = client
        .collection("facts")
        .find_one(&facts_id)
        .context("Failed to find facts")?;
    ensure!(facts_retrieved.is_some(), "facts record not found by id");
    println!("  ✓ facts: {}", facts_id);

    // Iterate find_many for episodes
    let mut episodes_count = 0;
    let cursor = client
        .collection("episodes")
        .find_many()
        .context("Failed to find_many episodes")?;
    for result in cursor {
        let doc = result.context("Failed to iterate cursor")?;
        ensure!(
            doc.data.get("tenant_id") == Some(&Bson::String("default".to_string())),
            "Episode missing envelope field: tenant_id"
        );
        ensure!(
            doc.data.get("namespace") == Some(&Bson::String("openclaw".to_string())),
            "Episode missing envelope field: namespace"
        );
        episodes_count += 1;
    }
    ensure!(
        episodes_count >= 1,
        "Expected at least 1 episode from find_many"
    );
    println!(
        "  ✓ find_many episodes: {} records with valid envelopes",
        episodes_count
    );
    println!("✅ Read path complete\n");

    // Step 8: Exercise promotion_path
    println!("🚀 Promotion path: insert derived fact with lineage...");

    let derived_fact_doc = doc! {
        "tenant_id": "default",
        "namespace": "openclaw",
        "entity_key": "derived-entity",
        "fact": "Derived fact from episode",
        "confidence": 0.92,
        "source_event_ids": [episodes_id.to_string()],
        "verified": true,
        "updated_at": "2026-04-24T00:00:00Z"
    };
    let derived_fact_id = client
        .collection("facts")
        .insert_one(derived_fact_doc.clone())
        .context("Failed to insert derived fact")?;
    println!("  ✓ derived_fact: {}", derived_fact_id);

    // Retrieve and verify lineage
    let derived_fact_retrieved = client
        .collection("facts")
        .find_one(&derived_fact_id)
        .context("Failed to find derived fact")?;
    ensure!(
        derived_fact_retrieved.is_some(),
        "Derived fact not found by id"
    );
    let derived_fact_doc_found = derived_fact_retrieved.unwrap();

    // Extract source_event_ids from the BSON document
    let source_event_ids = derived_fact_doc_found
        .data
        .get("source_event_ids")
        .context("source_event_ids field missing from derived fact")?;
    ensure!(
        !source_event_ids.as_array().unwrap_or(&vec![]).is_empty(),
        "source_event_ids array is empty"
    );

    // Verify the lineage reference exists in the array
    let event_ids_array = source_event_ids
        .as_array()
        .context("source_event_ids is not an array")?;
    let source_episode_id = episodes_id.to_string();
    let lineage_contains_episode = event_ids_array
        .iter()
        .any(|id| id.as_str() == Some(&source_episode_id));
    ensure!(
        lineage_contains_episode,
        "source_event_ids does not contain the episode id"
    );
    println!("  ✓ lineage verified: derived_fact contains episode reference");
    println!("✅ Promotion path complete\n");

    // Final summary
    println!("========================================");
    println!("✅ All smoke test paths passed:");
    println!("  ✓ Contract integrity (SHA-256)");
    println!("  ✓ Write path (4 collections)");
    println!("  ✓ Read path (find_one, find_many, envelope validation)");
    println!("  ✓ Promotion path (lineage references)");
    println!("✅ OpenClaw integration validated against locked SDK surface");
    println!("========================================");

    Ok(())
}
