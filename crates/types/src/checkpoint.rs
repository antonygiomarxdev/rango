use bson::Document;
use serde::{Deserialize, Serialize};

/// Checkpoint represents the last acknowledged mutation from the primary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Checkpoint(pub u64);

impl Checkpoint {
    pub fn initial() -> Self {
        Self(0)
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Snapshot unit for deterministic replay restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotUnit {
    pub snapshot_id: String,
    pub tenant_id: String,
    pub namespace: String,
    pub base_seq: u64,
    pub created_at: bson::DateTime,
    pub state: Vec<Document>,
}

/// Rollback request that targets a prior snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackUnit {
    pub snapshot_id: String,
    pub target_seq: u64,
    pub requested_at: bson::DateTime,
    pub reason: String,
}
