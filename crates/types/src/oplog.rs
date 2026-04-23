use crate::{Mutation, RollbackUnit, SnapshotUnit};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OplogEntry {
    pub seq: u64,
    pub timestamp: bson::DateTime,
    pub mutation: Mutation,
    pub origin: OplogOrigin,
    pub applied: bool,
    pub snapshot_anchor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OplogOrigin {
    Local,
    Remote,
    Replay,
}

/// Deterministic restore plan: hydrate snapshot, then replay operations after base_seq.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePlan {
    pub snapshot: SnapshotUnit,
    pub replay_from_seq: u64,
}

/// Audit record emitted when rollback is requested.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackAudit {
    pub rollback: RollbackUnit,
    pub applied_at: bson::DateTime,
}
