use rango_types::{Checkpoint, GovernanceDecision, Mutation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub node_id: String,
    pub tenant_id: String,
    pub namespace: String,
    pub mutations: Vec<Mutation>,
    pub last_checkpoint: Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub accepted_seqs: Vec<u64>,
    pub new_checkpoint: Checkpoint,
    #[serde(default)]
    pub rejected_non_owner_count: u64,
    #[serde(default)]
    pub rejected_cross_tenant_count: u64,
    #[serde(default)]
    pub audit: Vec<GovernanceDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub node_id: String,
    pub tenant_id: String,
    pub namespace: String,
    pub since_checkpoint: Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub mutations: Vec<Mutation>,
    pub new_checkpoint: Checkpoint,
    #[serde(default)]
    pub audit: Vec<GovernanceDecision>,
}
