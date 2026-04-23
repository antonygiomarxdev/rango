use std::str::FromStr;
use std::sync::Arc;

use bson::doc;
use rango_oplog::NullOplog;
use rango_sdk::RangoClient;
use rango_storage::MemoryStorage;
use rango_types::{
    ArtifactEnvelope, CollectionName, EventEnvelope, GovernanceMetadata, Mutation, MutationMetadata,
    MutationOp, RecordEnvelope, Revision,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("stateful agent wedge\n");

    let local = open_client("agent-node-a")?;
    let remote = open_client("agent-node-b")?;
    let collection = CollectionName::new("agent:memory");

    let event_payload = doc! {
        "session": "agent-001",
        "role": "user",
        "content": "Summarize yesterday's field notes"
    };

    let event_metadata = metadata("session-agent-001", "agent", "lineage-agent-001", "event", "agent-node-a")?;
    let event = EventEnvelope {
        metadata: event_metadata,
        write_id: "event-001".to_string(),
        sequence: 1,
        mutation_type: "insert".to_string(),
        mutation_data: Some(event_payload.clone()),
        is_tombstone: false,
    };
    event
        .validate()
        .map_err(|e| format!("event validation failed: {e}"))?;
    println!("[event] accepted write_id={} type={}", event.write_id, event.mutation_type);

    let memory = local.collection("agent:memory");
    let id = memory.insert_one(event_payload)?;
    let inserted = memory
        .find_one(&id)?
        .ok_or_else(|| "inserted document missing".to_string())?;

    let rev = inserted
        .data
        .get_str("_rev")
        .map_err(|_| "missing _rev on inserted record")?;
    let record = RecordEnvelope {
        metadata: metadata(&id.to_string(), "agent", "lineage-agent-001", "state", "agent-node-a")?,
        write_id: "record-001".to_string(),
        sequence: 1,
        data: inserted.data.clone(),
        conflict_siblings: vec![],
    };
    record
        .validate()
        .map_err(|e| format!("record validation failed: {e}"))?;
    println!(
        "[record] materialized id={} revision={}",
        record.metadata.id, rev
    );

    let artifact = ArtifactEnvelope {
        metadata: metadata(&id.to_string(), "agent", "lineage-agent-001", "artifact", "agent-node-a")?,
        write_id: "artifact-001".to_string(),
        artifact_type: "agent_summary".to_string(),
        source_revision: rev.to_string(),
        content: b"summary: field notes acknowledged".to_vec(),
        parent_artifact_revision: None,
    };
    artifact
        .validate()
        .map_err(|e| format!("artifact validation failed: {e}"))?;
    println!(
        "[artifact] derived type={} bytes={}",
        artifact.artifact_type,
        artifact.content.len()
    );

    println!("[sync] offline writes queued while link is down");
    memory.update_one(
        &id,
        doc! { "$set": { "status": "processed", "artifact_ref": "artifact-001" } },
    )?;
    let updated = memory
        .find_one(&id)?
        .ok_or_else(|| "updated document missing".to_string())?;

    let replay_batch = vec![
        mutation_from_doc(1, &record.data, "write-offline-001")?,
        mutation_from_doc(2, &updated.data, "write-offline-002")?,
    ];

    println!(
        "[sync] reconnect detected; replaying {} queued mutations",
        replay_batch.len()
    );
    remote
        .engine
        .apply_mutations_deterministic(&collection, replay_batch)?;

    let remote_view = remote
        .engine
        .find_one(&collection, &id)?
        .ok_or_else(|| "remote replay did not materialize record".to_string())?;
    println!(
        "[sync] reconciliation complete with status={}",
        remote_view.data.get_str("status").unwrap_or("unknown")
    );

    Ok(())
}

fn open_client(node_id: &str) -> Result<RangoClient<MemoryStorage>, Box<dyn std::error::Error>> {
    let storage = Arc::new(MemoryStorage::new());
    let oplog = Arc::new(NullOplog::new());
    Ok(RangoClient::open(storage, oplog, node_id)?)
}

fn metadata(
    id: &str,
    namespace: &str,
    lineage: &str,
    kind: &str,
    node: &str,
) -> Result<GovernanceMetadata, Box<dyn std::error::Error>> {
    let now = bson::DateTime::now();
    Ok(GovernanceMetadata {
        id: id.to_string(),
        namespace: namespace.to_string(),
        tenant_id: "tenant-a".to_string(),
        r#type: kind.to_string(),
        rev: Revision::now(node).to_string(),
        created_at: now,
        updated_at: now,
        source: node.to_string(),
        actor: node.to_string(),
        lineage: lineage.to_string(),
        schema_version: 1,
        trust_score: 1.0,
        verified: Some(true),
        expires_at: None,
    })
}

fn mutation_from_doc(
    seq: u64,
    doc: &bson::Document,
    write_id: &str,
) -> Result<Mutation, Box<dyn std::error::Error>> {
    let rev = doc
        .get_str("_rev")
        .map_err(|_| "missing _rev on replay document")?;
    let parsed_rev = Revision::from_str(rev)?;
    let doc_id = rango_types::DocumentId::from_bson(
        doc.get("_id")
            .ok_or_else(|| "missing _id on replay document")?
            .clone(),
    );

    Ok(Mutation {
        op: if seq == 1 {
            MutationOp::Insert
        } else {
            MutationOp::Update
        },
        collection: "agent:memory".to_string(),
        doc_id: doc_id.clone(),
        patch: Some(doc.clone()),
        seq,
        timestamp: bson::DateTime::now(),
        rev: parsed_rev.clone(),
        write_id: write_id.to_string(),
        metadata: MutationMetadata {
            id: doc_id.clone(),
            namespace: "agent:memory".to_string(),
            tenant_id: "tenant-a".to_string(),
            r#type: "state".to_string(),
            rev: parsed_rev,
            created_at: bson::DateTime::now(),
            updated_at: bson::DateTime::now(),
            source: "agent-node-a".to_string(),
            actor: "agent-node-a".to_string(),
            lineage: doc_id.to_string(),
            schema_version: 1,
            trust_score: 1.0,
            verified: Some(true),
            expires_at: None,
        },
    })
}
