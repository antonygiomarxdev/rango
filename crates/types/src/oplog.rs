use crate::Mutation;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OplogEntry {
    pub seq: u64,
    pub timestamp: bson::DateTime,
    pub mutation: Mutation,
    pub origin: OplogOrigin,
    pub applied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OplogOrigin {
    Local,
    Remote,
}
