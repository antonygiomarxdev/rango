use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use rand::RngCore;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use zeroize::Zeroize;

#[derive(Parser)]
#[command(name = "rango")]
#[command(about = "Rango — Local-first memory substrate CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Rango memory workspace
    Init {
        /// Path to create the workspace
        #[arg(default_value = ".rango")]
        path: PathBuf,
        /// Maximum document size in bytes (default: 16MB)
        #[arg(long)]
        max_doc_size: Option<usize>,
        /// Memory budget in bytes (default: 128MB)
        #[arg(long)]
        memory_budget: Option<usize>,
        /// Encryption passphrase (enables AES-256-GCM at rest)
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Inspect workspace status
    Inspect {
        /// Path to the workspace
        #[arg(default_value = ".rango")]
        path: PathBuf,
    },
    /// Import documents from a file
    Import {
        /// Path to the workspace
        #[arg(long, default_value = ".rango")]
        path: PathBuf,
        /// Collection name to import into
        #[arg(short, long)]
        collection: String,
        /// Path to the import file (JSON Lines)
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Import format
        #[arg(short, long, value_enum, default_value = "json")]
        format: ImportFormat,
        /// Encryption passphrase (required if workspace was initialized with encryption)
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Export documents to a file
    Export {
        /// Path to the workspace
        #[arg(long, default_value = ".rango")]
        path: PathBuf,
        /// Collection name to export from
        #[arg(short, long)]
        collection: String,
        /// Path to the output file
        #[arg(short, long, default_value = "export.json")]
        output: PathBuf,
        /// Export format
        #[arg(short, long, value_enum, default_value = "json")]
        format: ExportFormat,
        /// Encryption passphrase (required if workspace was initialized with encryption)
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Run benchmarks
    Bench {
        /// Number of documents to use in benchmarks
        #[arg(short, long, default_value = "10000")]
        count: usize,
    },
    /// Run diagnostics
    Doctor {
        /// Path to the workspace
        #[arg(default_value = ".rango")]
        path: PathBuf,
        /// Encryption passphrase (required if workspace was initialized with encryption)
        #[arg(long)]
        passphrase: Option<String>,
    },
    /// Sync with a remote server (one-shot push/pull)
    Sync {
        /// Path to the workspace
        #[arg(default_value = ".rango")]
        path: PathBuf,
        /// Server URL
        #[arg(short, long)]
        server: String,
        /// Auth token
        #[arg(short, long)]
        token: String,
        /// Node identifier
        #[arg(short, long, default_value = "cli-node")]
        node_id: String,
        /// Encryption passphrase (required if workspace was initialized with encryption)
        #[arg(long)]
        passphrase: Option<String>,
    },
}

#[derive(Clone, ValueEnum)]
enum ImportFormat {
    Json,
}

#[derive(Clone, ValueEnum)]
enum ExportFormat {
    Json,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            path,
            max_doc_size,
            memory_budget,
            passphrase,
        } => {
            let path = sanitize_path(&path)?;
            std::fs::create_dir_all(&path)?;
            let config = rango_types::RangoConfig {
                max_document_size_bytes: max_doc_size.unwrap_or(16 * 1024 * 1024),
                memory_budget_bytes: memory_budget.unwrap_or(128 * 1024 * 1024),
            };
            let config_path = path.join("rango.json");
            let config_json = serde_json::json!({
                "max_document_size_bytes": config.max_document_size_bytes,
                "memory_budget_bytes": config.memory_budget_bytes,
            });
            std::fs::write(&config_path, config_json.to_string())?;

            // Initialize persistent storage backend (redb) and oplog files.
            let _storage = rango_storage::RedbStorage::open(path.join("data.redb"))?;
            let _oplog = rango_oplog::FileOplog::new(path.join("oplog.rgo"))?;

            if let Some(mut pass) = passphrase {
                let mut salt = [0u8; 16];
                rand::thread_rng().fill_bytes(&mut salt);
                let salt_path = path.join("salt");
                std::fs::write(&salt_path, salt)?;
                // Verify round-trip
                let crypto = rango_storage::CryptoEngine::from_passphrase(&pass, &salt);
                let test = crypto.encrypt(b"test");
                assert!(
                    crypto.decrypt(&test).is_ok(),
                    "encryption round-trip failed"
                );
                pass.zeroize();
                println!("  Encryption: AES-256-GCM enabled");
            } else {
                println!("  Encryption: disabled");
            }

            println!("Initialized Rango memory workspace at {}", path.display());
            println!(
                "  Max document size: {} bytes",
                config.max_document_size_bytes
            );
            println!("  Memory budget: {} bytes", config.memory_budget_bytes);
            println!("  Storage engine: redb (default)");
        }
        Commands::Inspect { path } => {
            let path = sanitize_path(&path)?;
            if !path.exists() {
                anyhow::bail!("Workspace not found at {}", path.display());
            }
            println!("Rango memory workspace at {}", path.display());
            println!("  Status: OK");

            let config = load_config(&path);
            println!(
                "  Max document size: {} bytes",
                config.max_document_size_bytes
            );
            println!("  Memory budget: {} bytes", config.memory_budget_bytes);

            let _client = open_persistent_client(&path, "cli-node", None)?;
            let data_path = path.join("data.redb");
            if data_path.exists() {
                let size = std::fs::metadata(&data_path)?.len();
                println!("  Storage file: {} ({} bytes)", data_path.display(), size);
            }

            // For now, just show placeholder
            println!("  Collections: (not yet trackable)");
            println!("  Documents: (not yet trackable)");
        }
        Commands::Import {
            path,
            collection,
            file,
            format: _,
            passphrase,
        } => {
            let path = sanitize_path(&path)?;
            if !file.exists() {
                anyhow::bail!("Import file not found: {}", file.display());
            }

            let client = open_persistent_client(&path, "cli-node", passphrase.as_deref())?;

            println!(
                "Importing into collection '{}' from {}...",
                collection,
                file.display()
            );
            let progress = rango_sdk::migrate::ConsoleProgress;
            let result = client.import_json(&collection, &file, &progress)?;

            println!(
                "Import complete: {} documents imported, {} errors",
                result.imported, result.errors
            );
        }
        Commands::Export {
            path,
            collection,
            output,
            format: _,
            passphrase,
        } => {
            let path = sanitize_path(&path)?;
            let client = open_persistent_client(&path, "cli-node", passphrase.as_deref())?;

            println!(
                "Exporting collection '{}' to {}...",
                collection,
                output.display()
            );
            let result = client.export_json(&collection, &output)?;

            println!("Export complete: {} documents exported", result.exported);
        }
        Commands::Bench { count } => {
            run_benchmarks(count)?;
        }
        Commands::Doctor { path, passphrase } => {
            let path = sanitize_path(&path)?;
            run_doctor(&path, passphrase.as_deref())?;
        }
        Commands::Sync {
            path,
            server,
            token,
            node_id,
            passphrase,
        } => {
            let path = sanitize_path(&path)?;
            run_sync(&path, &server, &token, &node_id, passphrase.as_deref()).await?;
        }
    }

    Ok(())
}

fn run_benchmarks(count: usize) -> Result<()> {
    use bson::doc;
    use rango_types::CollectionName;

    println!("Rango Benchmarks ({} docs)", count);
    println!("{}", "=".repeat(50));

    let workspace = std::env::temp_dir().join(format!(
        "rango-cli-bench-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&workspace)?;
    let storage = Arc::new(rango_storage::RedbStorage::open(
        workspace.join("data.redb"),
    )?);
    let oplog = Arc::new(rango_oplog::FileOplog::new(workspace.join("oplog.rgo"))?);
    let client = rango_sdk::RangoClient::open(storage.clone(), oplog, "bench-node")?;
    let coll = CollectionName::new("bench");
    println!("  Backend: redb (persistent)");
    println!("  Workspace: {}", workspace.display());

    // Insert benchmark
    println!("\nInsert Benchmark");
    let start = Instant::now();
    for i in 0..count {
        let doc = doc! { "index": i as i64, "name": format!("user-{}", i) };
        client.engine.insert_one(&coll, doc)?;
    }
    let insert_duration = start.elapsed();
    let insert_rate = count as f64 / insert_duration.as_secs_f64();
    println!("  Inserted {} docs in {:?}", count, insert_duration);
    println!("  Throughput: {:.0} docs/sec", insert_rate);

    // Find by _id benchmark
    println!("\nFind-by-ID Benchmark");
    let ids: Vec<rango_types::DocumentId> = client
        .engine
        .find_many(&coll)?
        .filter_map(|r: Result<rango_types::RangoDocument, rango_types::RangoError>| r.ok())
        .take(1000)
        .map(|d| rango_types::DocumentId::from_bson(d.data.get("_id").unwrap().clone()))
        .collect();

    let start = Instant::now();
    for id in &ids {
        let _ = client.engine.find_one(&coll, id)?;
    }
    let find_duration = start.elapsed();
    let find_rate = ids.len() as f64 / find_duration.as_secs_f64();
    println!("  Queried {} docs in {:?}", ids.len(), find_duration);
    println!("  Throughput: {:.0} queries/sec", find_rate);
    println!(
        "  Avg latency: {:.3} ms",
        find_duration.as_secs_f64() * 1000.0 / ids.len() as f64
    );

    // Filter benchmark
    println!("\nFilter Benchmark (find by field)");
    let start = Instant::now();
    let cursor = client.engine.find(
        &coll,
        &doc! { "index": { "$gte": (count / 2) as i64 } },
        None,
        None,
        None,
        None,
    )?;
    let filtered_count = cursor
        .filter_map(|r: Result<rango_types::RangoDocument, rango_types::RangoError>| r.ok())
        .count();
    let filter_duration = start.elapsed();
    println!(
        "  Filtered {} docs in {:?}",
        filtered_count, filter_duration
    );
    println!(
        "  Throughput: {:.0} docs/sec",
        filtered_count as f64 / filter_duration.as_secs_f64()
    );

    // Summary
    println!("\n{}", "=".repeat(50));
    println!("Summary");
    println!("  Insert: {:.0} docs/sec", insert_rate);
    println!(
        "  Find-by-ID: {:.0} queries/sec ({:.3} ms avg)",
        find_rate,
        find_duration.as_secs_f64() * 1000.0 / ids.len() as f64
    );
    println!(
        "  Filter: {:.0} docs/sec",
        filtered_count as f64 / filter_duration.as_secs_f64()
    );

    Ok(())
}

fn run_doctor(path: &Path, passphrase: Option<&str>) -> Result<()> {
    use bson::doc;
    use rango_oplog::Oplog;
    use rango_sync::{CheckpointStore, SyncQueue};
    use rango_types::CollectionName;

    println!("Rango Doctor");
    println!("{}", "=".repeat(50));

    // Check workspace directory
    println!("\nStorage Check");
    if path.exists() {
        println!("  [OK] Workspace directory exists: {}", path.display());
    } else {
        println!("  [WARN] Workspace directory not found: {}", path.display());
    }

    // Config check
    println!("\nConfig Check");
    let config = load_config(path);
    println!(
        "  Max document size: {} bytes",
        config.max_document_size_bytes
    );
    println!("  Memory budget: {} bytes", config.memory_budget_bytes);

    let crypto = load_crypto(path, passphrase)?;
    if crypto.is_some() {
        println!("  Encryption: AES-256-GCM enabled");
    } else {
        println!("  Encryption: disabled");
    }

    // Try to open engine
    println!("\nEngine Check");
    let client = match open_persistent_client(path, "doctor-node", passphrase) {
        Ok(c) => {
            println!("  [OK] Engine opened successfully");
            c
        }
        Err(e) => {
            println!("  [FAIL] Failed to open engine: {}", e);
            return Ok(());
        }
    };

    // Basic operations test
    println!("\nOperations Check");
    let coll = CollectionName::new("__doctor_test");
    let id = client.engine.insert_one(&coll, doc! { "test": true })?;
    println!("  [OK] Insert works");

    let found = client.engine.find_one(&coll, &id)?;
    assert!(found.is_some());
    println!("  [OK] Find-by-ID works");

    let updated = client
        .engine
        .update_one(&coll, &id, doc! { "$set": { "test": false } })?;
    assert!(updated);
    println!("  [OK] Update works");

    let deleted = client.engine.delete_one(&coll, &id)?;
    assert!(deleted);
    println!("  [OK] Delete (tombstone) works");

    // Check metadata
    println!("\nMetadata Check");
    let id2 = client.engine.insert_one(&coll, doc! { "name": "doctor" })?;
    let doc = client.engine.find_one(&coll, &id2)?.unwrap();
    let has_rev = doc.data.contains_key("_rev");
    let has_updated_at = doc.data.contains_key("_updated_at");
    let has_source_node = doc.data.contains_key("_source_node");

    if has_rev {
        println!("  [OK] _rev field present");
    } else {
        println!("  [WARN] _rev field missing");
    }
    if has_updated_at {
        println!("  [OK] _updated_at field present");
    } else {
        println!("  [WARN] _updated_at field missing");
    }
    if has_source_node {
        println!("  [OK] _source_node field present");
    } else {
        println!("  [WARN] _source_node field missing");
    }

    // Cleanup test collection
    let _ = client.engine.delete_one(&coll, &id2);

    // Show metrics
    println!("\nMetrics Snapshot");
    let metrics = client.engine.metrics().snapshot();
    println!("  Inserts: {}", metrics.inserts);
    println!("  Finds: {}", metrics.finds);
    println!("  Updates: {}", metrics.updates);
    println!("  Deletes: {}", metrics.deletes);
    println!("  Sync pushes: {}", metrics.sync_pushes);
    println!("  Sync pulls: {}", metrics.sync_pulls);

    // Sync infrastructure check
    println!("\nSync Infrastructure Check");
    let oplog_path = path.join("oplog.rgo");
    let queue_path = path.join("sync-queue.rgo");
    let checkpoint_path = path.join("checkpoint.json");

    if oplog_path.exists() {
        match rango_oplog::FileOplog::new_with_crypto(&oplog_path, crypto.clone()) {
            Ok(oplog) => {
                let seq = oplog.latest_seq().unwrap_or(0);
                println!("  [OK] Oplog exists (latest seq: {})", seq);
            }
            Err(e) => println!("  [WARN] Oplog exists but cannot open: {}", e),
        }
    } else {
        println!("  [INFO] Oplog not found (expected for new workspaces)");
    }

    if queue_path.exists() {
        match rango_sync::FileSyncQueue::new_with_crypto(&queue_path, crypto.clone()) {
            Ok(queue) => {
                let batch = queue.next_batch(1000).unwrap_or_default();
                let pending = batch
                    .iter()
                    .filter(|e| matches!(e.state, rango_types::QueueState::Pending))
                    .count();
                let inflight = batch
                    .iter()
                    .filter(|e| matches!(e.state, rango_types::QueueState::Inflight))
                    .count();
                let failed = batch
                    .iter()
                    .filter(|e| matches!(e.state, rango_types::QueueState::Failed))
                    .count();
                println!(
                    "  [OK] Sync queue exists (pending: {}, inflight: {}, failed: {})",
                    pending, inflight, failed
                );
            }
            Err(e) => println!("  [WARN] Sync queue exists but cannot open: {}", e),
        }
    } else {
        println!("  [INFO] Sync queue not found (expected for new workspaces)");
    }

    if checkpoint_path.exists() {
        match rango_sync::FileCheckpointStore::new_with_crypto(&checkpoint_path, crypto.clone())
            .get()
        {
            Ok(cp) => println!("  [OK] Checkpoint exists (last_seq: {})", cp.0),
            Err(e) => println!("  [WARN] Checkpoint exists but cannot read: {}", e),
        }
    } else {
        println!("  [INFO] Checkpoint not found (expected for new workspaces)");
    }

    println!("\n{}", "=".repeat(50));
    println!("Doctor check complete.");

    Ok(())
}

fn sanitize_path(path: &Path) -> Result<PathBuf, anyhow::Error> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        anyhow::bail!("Path cannot contain parent directory references (..)");
    }
    Ok(path)
}

fn load_crypto(
    path: &Path,
    passphrase: Option<&str>,
) -> Result<Option<Arc<rango_storage::CryptoEngine>>, anyhow::Error> {
    let salt_path = path.join("salt");
    if salt_path.exists() {
        let salt = std::fs::read(&salt_path)?;
        let pass = passphrase
            .ok_or_else(|| anyhow::anyhow!("Workspace is encrypted. Passphrase required."))?;
        Ok(Some(Arc::new(
            rango_storage::CryptoEngine::from_passphrase(pass, &salt),
        )))
    } else {
        if passphrase.is_some() {
            return Err(anyhow::anyhow!(
                "Passphrase provided but workspace is not encrypted."
            ));
        }
        Ok(None)
    }
}

fn open_persistent_client(
    path: &Path,
    node_id: &str,
    passphrase: Option<&str>,
) -> Result<rango_sdk::RangoClient<rango_storage::RedbStorage>, anyhow::Error> {
    let config = load_config(path);
    let storage = Arc::new(rango_storage::RedbStorage::open(path.join("data.redb"))?);
    let crypto = load_crypto(path, passphrase)?;
    let oplog = Arc::new(rango_oplog::FileOplog::new_with_crypto(
        path.join("oplog.rgo"),
        crypto,
    )?);
    let client = rango_sdk::RangoClient::open_with_config(storage, oplog, node_id, config)?;
    Ok(client)
}

fn load_config(path: &Path) -> rango_types::RangoConfig {
    let config_path = path.join("rango.json");
    if let Ok(contents) = std::fs::read_to_string(&config_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
            return rango_types::RangoConfig {
                max_document_size_bytes: json
                    .get("max_document_size_bytes")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(16 * 1024 * 1024),
                memory_budget_bytes: json
                    .get("memory_budget_bytes")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
                    .unwrap_or(128 * 1024 * 1024),
            };
        }
    }
    rango_types::RangoConfig::default()
}

async fn run_sync(
    path: &Path,
    server: &str,
    token: &str,
    node_id: &str,
    passphrase: Option<&str>,
) -> Result<()> {
    use rango_oplog::{FileOplog, Oplog};
    use rango_sync::{
        CheckpointStore, FileCheckpointStore, FileSyncQueue, SyncClient, SyncConfig, SyncQueue,
        SyncScheduler,
    };

    println!("Rango Sync");
    println!("{}", "=".repeat(50));

    if !path.exists() {
        anyhow::bail!("Workspace not found at {}", path.display());
    }

    let crypto = load_crypto(path, passphrase)?;

    // Open file-based sync infrastructure
    let oplog_path = path.join("oplog.rgo");
    let queue_path = path.join("sync-queue.rgo");
    let checkpoint_path = path.join("checkpoint.json");

    let oplog = FileOplog::new_with_crypto(&oplog_path, crypto.clone())?;
    let queue = FileSyncQueue::new_with_crypto(&queue_path, crypto.clone())?;
    let checkpoint_store = FileCheckpointStore::new_with_crypto(&checkpoint_path, crypto.clone());
    let client = SyncClient::new(server, token);
    let scheduler = SyncScheduler::new(SyncConfig::default());

    // Enqueue any oplog entries not yet in the queue
    let checkpoint = checkpoint_store.get()?;
    let mut seq = checkpoint.0 + 1;
    let mut enqueued = 0;
    loop {
        let entries = oplog.read_since(seq, 100)?;
        if entries.is_empty() {
            break;
        }
        for entry in entries {
            queue.enqueue(entry.seq)?;
            seq = entry.seq + 1;
            enqueued += 1;
        }
    }
    if enqueued > 0 {
        println!("Enqueued {} local mutations for sync", enqueued);
    }

    // Run one sync cycle
    match scheduler
        .run_once(node_id, &queue, &oplog, &checkpoint_store, &client)
        .await
    {
        Ok(result) => {
            println!("Push: {} mutations", result.pushed);
            println!("Pull: {} mutations", result.pulled);
        }
        Err(e) => {
            anyhow::bail!("Sync failed: {}", e);
        }
    }

    println!("\n{}", "=".repeat(50));
    println!("Sync complete.");

    Ok(())
}
