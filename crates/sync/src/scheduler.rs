use std::time::Duration;

use rango_oplog::Oplog;
use rango_types::RangoError;
use tracing::{error, info, instrument, warn};

use crate::checkpoint::CheckpointStore;
use crate::client::SyncClient;
use crate::queue::SyncQueue;

/// Sync scheduler configuration.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub batch_size: usize,
    pub poll_interval: Duration,
    pub max_retries: u32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            poll_interval: Duration::from_secs(5),
            max_retries: 10,
        }
    }
}

/// One-shot sync scheduler.
pub struct SyncScheduler {
    config: SyncConfig,
}

impl SyncScheduler {
    pub fn new(config: SyncConfig) -> Self {
        Self { config }
    }

    #[instrument(skip(self, queue, oplog, checkpoint_store, client), fields(node_id))]
    pub async fn run_once(
        &self,
        node_id: &str,
        queue: &dyn SyncQueue,
        oplog: &dyn Oplog,
        checkpoint_store: &dyn CheckpointStore,
        client: &SyncClient,
    ) -> Result<SyncResult, RangoError> {
        let mut result = SyncResult::default();

        // ---- PUSH ----
        let batch = queue.next_batch(self.config.batch_size)?;
        if !batch.is_empty() {
            let seqs: Vec<u64> = batch.iter().map(|e| e.seq).collect();
            let entries = oplog.read_since(seqs[0], seqs.len())?;
            let mutations: Vec<_> = entries.into_iter().map(|e| e.mutation).collect();
            let checkpoint = checkpoint_store.get()?;

            match client.push(node_id, mutations, checkpoint).await {
                Ok(resp) => {
                    info!(
                        "Pushed {} mutations, checkpoint {:?}",
                        seqs.len(),
                        resp.new_checkpoint
                    );
                    queue.mark_acked(&resp.accepted_seqs)?;
                    checkpoint_store.set(resp.new_checkpoint)?;
                    result.pushed = resp.accepted_seqs.len();
                }
                Err(e) => {
                    error!("Push failed: {}", e);
                    for seq in seqs {
                        queue.mark_failed(seq, e.to_string())?;
                    }
                }
            }
        }

        // ---- PULL ----
        let checkpoint = checkpoint_store.get()?;
        match client.pull(node_id, checkpoint).await {
            Ok(resp) => {
                info!(
                    "Pulled {} mutations, checkpoint {:?}",
                    resp.mutations.len(),
                    resp.new_checkpoint
                );
                result.pulled = resp.mutations.len();
                checkpoint_store.set(resp.new_checkpoint)?;
            }
            Err(e) => {
                warn!("Pull failed: {}", e);
            }
        }

        Ok(result)
    }
}

#[derive(Debug, Default)]
pub struct SyncResult {
    pub pushed: usize,
    pub pulled: usize,
}
