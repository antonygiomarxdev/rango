use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{extract::Json, http::StatusCode, Extension};
use rango_oplog::Oplog;
use rango_types::{Checkpoint, Mutation, OplogEntry, OplogOrigin, RangoError};
use rango_sync::protocol::{PullRequest, PullResponse, PushRequest, PushResponse};
use tracing::{info, instrument, warn};

pub struct ServerState {
    pub oplog: Arc<dyn Oplog>,
    pub tokens: Mutex<HashMap<String, String>>, // token -> node_id
}

impl ServerState {
    pub fn new(oplog: Arc<dyn Oplog>) -> Self {
        Self {
            oplog,
            tokens: Mutex::new(HashMap::new()),
        }
    }

    pub fn add_token(&self, token: impl Into<String>, node_id: impl Into<String>) {
        self.tokens.lock().unwrap().insert(token.into(), node_id.into());
    }

    fn validate_token(&self, auth_header: Option<&str>) -> Option<String> {
        let token = auth_header?.strip_prefix("Bearer ")?;
        self.tokens.lock().unwrap().get(token).cloned()
    }
}

#[instrument(skip(state, req))]
pub async fn handle_push(
    Extension(state): Extension<Arc<ServerState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<PushRequest>,
) -> Result<Json<PushResponse>, StatusCode> {
    info!(mutations = req.mutations.len(), "handling push");
    // Validate protocol version
    let protocol_version = headers.get("X-Rango-Protocol-Version")
        .and_then(|v| v.to_str().ok());
    if protocol_version != Some("1") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate auth token
    let auth = headers.get("Authorization")
        .and_then(|v| v.to_str().ok());
    let _node_id = state.validate_token(auth)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let mut accepted_seqs = Vec::new();
    for mutation in req.mutations {
        let seq = append_mutation(&state, mutation).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        accepted_seqs.push(seq);
    }

    let new_checkpoint = Checkpoint(state.oplog.latest_seq().unwrap_or(0));

    Ok(Json(PushResponse {
        accepted_seqs,
        new_checkpoint,
    }))
}

#[instrument(skip(state, req))]
pub async fn handle_pull(
    Extension(state): Extension<Arc<ServerState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<PullRequest>,
) -> Result<Json<PullResponse>, StatusCode> {
    info!(since = req.since_checkpoint.0, "handling pull");
    // Validate protocol version
    let protocol_version = headers.get("X-Rango-Protocol-Version")
        .and_then(|v| v.to_str().ok());
    if protocol_version != Some("1") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate auth token
    let auth = headers.get("Authorization")
        .and_then(|v| v.to_str().ok());
    let _node_id = state.validate_token(auth)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let entries = state.oplog
        .read_since(req.since_checkpoint.0 + 1, 1000)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mutations: Vec<Mutation> = entries.into_iter().map(|e| e.mutation).collect();
    let new_checkpoint = Checkpoint(state.oplog.latest_seq().unwrap_or(0));

    Ok(Json(PullResponse {
        mutations,
        new_checkpoint,
    }))
}

fn append_mutation(state: &ServerState, mutation: Mutation) -> Result<u64, RangoError> {
    // Simple idempotency check: deduplicate by write_id
    // For MVP, scan recent entries (last 1000) to check for duplicate write_id
    let latest = state.oplog.latest_seq()?;
    let since = if latest > 1000 { latest - 1000 } else { 0 };
    let recent = state.oplog.read_since(since, 1000)?;
    for entry in recent {
        if entry.mutation.write_id == mutation.write_id {
            return Ok(entry.seq); // Already exists
        }
    }

    let entry = OplogEntry {
        seq: 0,
        timestamp: bson::DateTime::now(),
        mutation,
        origin: OplogOrigin::Remote,
        applied: false,
    };
    state.oplog.append(entry)
}
