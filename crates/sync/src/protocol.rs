use rango_types::{Checkpoint, Mutation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub node_id: String,
    pub mutations: Vec<Mutation>,
    pub last_checkpoint: Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub accepted_seqs: Vec<u64>,
    pub new_checkpoint: Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub node_id: String,
    pub since_checkpoint: Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub mutations: Vec<Mutation>,
    pub new_checkpoint: Checkpoint,
}
