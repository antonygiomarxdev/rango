use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::{Extension, extract::Json, http::StatusCode};
use bson::Document;
use rango_core::{
    ControlPlane, PromotionRequest as CorePromotionRequest, ReadRequest, WriteContext, WritePayload,
};
use rango_oplog::Oplog;
use rango_sync::protocol::{
    PromoteRequest, PromoteResponse, PullRequest, PullResponse, PushRequest, PushResponse,
};
use rango_types::{
    Checkpoint, GovernanceDecision, MemoryTier, Mutation, OplogEntry, OplogOrigin, PolicyDecision,
    RangoError,
};
use tracing::{info, instrument};

#[derive(Debug, Clone)]
pub struct AuthPrincipal {
    pub node_id: String,
    pub tenant_id: String,
}

pub struct ServerState {
    pub oplog: Arc<dyn Oplog>,
    pub tokens: Mutex<HashMap<String, AuthPrincipal>>, // token -> principal
    pub non_owner_rejections: AtomicU64,
    pub cross_tenant_rejections: AtomicU64,
    pub control_plane: Arc<ControlPlane>,
}

impl ServerState {
    pub fn new(oplog: Arc<dyn Oplog>) -> Self {
        Self {
            oplog,
            tokens: Mutex::new(HashMap::new()),
            non_owner_rejections: AtomicU64::new(0),
            cross_tenant_rejections: AtomicU64::new(0),
            control_plane: Arc::new(ControlPlane::default()),
        }
    }

    pub fn add_token(&self, token: impl Into<String>, node_id: impl Into<String>) {
        self.add_token_with_tenant(token, node_id, "default");
    }

    pub fn add_token_with_tenant(
        &self,
        token: impl Into<String>,
        node_id: impl Into<String>,
        tenant_id: impl Into<String>,
    ) {
        self.tokens.lock().unwrap().insert(
            token.into(),
            AuthPrincipal {
                node_id: node_id.into(),
                tenant_id: tenant_id.into(),
            },
        );
    }

    fn validate_token(&self, auth_header: Option<&str>) -> Option<AuthPrincipal> {
        let token = auth_header?.strip_prefix("Bearer ")?;
        self.tokens.lock().unwrap().get(token).cloned()
    }

    pub fn non_owner_rejections(&self) -> u64 {
        self.non_owner_rejections.load(Ordering::Relaxed)
    }

    pub fn cross_tenant_rejections(&self) -> u64 {
        self.cross_tenant_rejections.load(Ordering::Relaxed)
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
    let protocol_version = headers
        .get("X-Rango-Protocol-Version")
        .and_then(|v| v.to_str().ok());
    if protocol_version != Some("1") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate auth token
    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok());
    let principal = state.validate_token(auth).ok_or(StatusCode::UNAUTHORIZED)?;

    if req.node_id != principal.node_id {
        state.non_owner_rejections.fetch_add(1, Ordering::Relaxed);
        let new_checkpoint = Checkpoint(state.oplog.latest_seq().unwrap_or(0));
        return Ok(Json(PushResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint,
            rejected_non_owner_count: 1,
            rejected_cross_tenant_count: 0,
            audit: vec![GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "node_mismatch".to_string(),
            }],
        }));
    }

    if req.tenant_id != principal.tenant_id {
        state
            .cross_tenant_rejections
            .fetch_add(1, Ordering::Relaxed);
        let new_checkpoint = Checkpoint(state.oplog.latest_seq().unwrap_or(0));
        return Ok(Json(PushResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint,
            rejected_non_owner_count: 0,
            rejected_cross_tenant_count: 1,
            audit: vec![GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "tenant_mismatch".to_string(),
            }],
        }));
    }

    let mut accepted_seqs = Vec::new();
    let mut rejected_cross_tenant_count = 0u64;
    let mut audit = Vec::new();
    for mutation in req.mutations {
        if let Err(err) = mutation.validate_metadata() {
            audit.push(GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: format!("invalid_metadata:{err}"),
            });
            continue;
        }

        if mutation.metadata.tenant_id != req.tenant_id
            || mutation.metadata.namespace != req.namespace
        {
            state
                .cross_tenant_rejections
                .fetch_add(1, Ordering::Relaxed);
            rejected_cross_tenant_count += 1;
            audit.push(GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "cross_tenant_or_namespace_mutation".to_string(),
            });
            continue;
        }

        let write_ctx = WriteContext {
            tenant_id: req.tenant_id.clone(),
            namespace: req.namespace.clone(),
            actor: mutation.metadata.actor.clone(),
            source: mutation.metadata.source.clone(),
            tier: MemoryTier::State,
        };
        let payload = WritePayload::StateWithTrust {
            document: mutation.patch.clone().unwrap_or_else(Document::new),
            trust_score: mutation.metadata.trust_score,
        };
        let decision = state
            .control_plane
            .write_path(&write_ctx, &payload)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if matches!(decision.decision, PolicyDecision::Reject) {
            audit.push(decision);
            continue;
        }

        let seq = append_mutation(&state, mutation, &req.tenant_id, &req.namespace)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        accepted_seqs.push(seq);
        audit.push(decision);
    }

    let new_checkpoint = Checkpoint(state.oplog.latest_seq().unwrap_or(0));

    Ok(Json(PushResponse {
        accepted_seqs,
        new_checkpoint,
        rejected_non_owner_count: 0,
        rejected_cross_tenant_count,
        audit,
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
    let protocol_version = headers
        .get("X-Rango-Protocol-Version")
        .and_then(|v| v.to_str().ok());
    if protocol_version != Some("1") {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate auth token
    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok());
    let principal = state.validate_token(auth).ok_or(StatusCode::UNAUTHORIZED)?;
    if principal.node_id != req.node_id || principal.tenant_id != req.tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let entries = state
        .oplog
        .read_since(req.since_checkpoint.0 + 1, 1000)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let scoped_entries: Vec<OplogEntry> = entries
        .into_iter()
        .filter(|e| {
            e.mutation.metadata.tenant_id == req.tenant_id
                && e.mutation.metadata.namespace == req.namespace
        })
        .collect();

    let candidates: Vec<Document> = scoped_entries
        .iter()
        .filter_map(|e| e.mutation.patch.clone())
        .collect();
    let read_request = ReadRequest {
        tenant_id: req.tenant_id.clone(),
        namespace: req.namespace.clone(),
        tier: MemoryTier::State,
        limit: 1000,
    };
    let (read_decision, filtered_candidates) = state
        .control_plane
        .read_path(&read_request, candidates)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if matches!(read_decision.decision, PolicyDecision::Reject) {
        return Ok(Json(PullResponse {
            mutations: Vec::new(),
            new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(0)),
            audit: vec![read_decision],
        }));
    }

    let mut allowed_patch_counts: HashMap<Vec<u8>, usize> = HashMap::new();
    for candidate in &filtered_candidates {
        let key = document_key(candidate);
        *allowed_patch_counts.entry(key).or_insert(0) += 1;
    }

    let mut mutations: Vec<Mutation> = Vec::new();
    for entry in scoped_entries {
        let Some(patch) = entry.mutation.patch.as_ref() else {
            continue;
        };
        let key = document_key(patch);
        let Some(remaining) = allowed_patch_counts.get_mut(&key) else {
            continue;
        };
        if *remaining > 0 {
            mutations.push(entry.mutation);
            *remaining -= 1;
        }
    }
    let new_checkpoint = Checkpoint(state.oplog.latest_seq().unwrap_or(0));

    Ok(Json(PullResponse {
        mutations,
        new_checkpoint,
        audit: vec![read_decision],
    }))
}

#[instrument(skip(state, req))]
pub async fn handle_promote(
    Extension(state): Extension<Arc<ServerState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<PromoteRequest>,
) -> Result<Json<PromoteResponse>, StatusCode> {
    info!(candidate_id = req.candidate_id, "handling promote");
    let protocol_version = headers
        .get("X-Rango-Protocol-Version")
        .and_then(|v| v.to_str().ok());
    if protocol_version != Some("1") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok());
    let principal = state.validate_token(auth).ok_or(StatusCode::UNAUTHORIZED)?;

    if req.node_id != principal.node_id {
        state.non_owner_rejections.fetch_add(1, Ordering::Relaxed);
        return Ok(Json(PromoteResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(0)),
            rejected_count: 1,
            audit: vec![GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "node_mismatch".to_string(),
            }],
        }));
    }

    if req.tenant_id != principal.tenant_id {
        state.cross_tenant_rejections.fetch_add(1, Ordering::Relaxed);
        return Ok(Json(PromoteResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(0)),
            rejected_count: 1,
            audit: vec![GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "tenant_mismatch".to_string(),
            }],
        }));
    }

    if let Err(err) = req.mutation.validate_metadata() {
        return Ok(Json(PromoteResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(0)),
            rejected_count: 1,
            audit: vec![GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: format!("invalid_metadata:{err}"),
            }],
        }));
    }

    if req.mutation.metadata.tenant_id != req.tenant_id || req.mutation.metadata.namespace != req.namespace
    {
        state.cross_tenant_rejections.fetch_add(1, Ordering::Relaxed);
        return Ok(Json(PromoteResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(0)),
            rejected_count: 1,
            audit: vec![GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "cross_tenant_or_namespace_mutation".to_string(),
            }],
        }));
    }

    let promotion_request = CorePromotionRequest {
        tenant_id: req.tenant_id.clone(),
        namespace: req.namespace.clone(),
        from: req.from_tier,
        to: req.to_tier,
        candidate_id: req.candidate_id.clone(),
    };
    let payload = WritePayload::StateWithTrust {
        document: req.mutation.patch.clone().unwrap_or_else(Document::new),
        trust_score: req.mutation.metadata.trust_score,
    };

    let (decision, _) = state
        .control_plane
        .promotion_path(&promotion_request, &payload)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !matches!(decision.decision, PolicyDecision::Allow) {
        return Ok(Json(PromoteResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(0)),
            rejected_count: 1,
            audit: vec![decision],
        }));
    }

    let seq = append_mutation(&state, req.mutation, &req.tenant_id, &req.namespace)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(PromoteResponse {
        accepted_seqs: vec![seq],
        new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(seq)),
        rejected_count: 0,
        audit: vec![decision],
    }))
}

fn append_mutation(
    state: &ServerState,
    mutation: Mutation,
    tenant_id: &str,
    namespace: &str,
) -> Result<u64, RangoError> {
    // Simple idempotency check: deduplicate by write_id
    // Dedup key is tenant + namespace + write_id for isolation safety.
    let latest = state.oplog.latest_seq()?;
    let all = state.oplog.read_since(1, latest as usize + 1)?;
    for entry in all {
        if entry.mutation.write_id == mutation.write_id
            && entry.mutation.metadata.tenant_id == tenant_id
            && entry.mutation.metadata.namespace == namespace
        {
            return Ok(entry.seq); // Already exists
        }
    }

    let entry = OplogEntry {
        seq: 0,
        timestamp: bson::DateTime::now(),
        mutation,
        origin: OplogOrigin::Remote,
        applied: false,
        snapshot_anchor: None,
    };
    state.oplog.append(entry)
}

fn document_key(doc: &Document) -> Vec<u8> {
    format!("{doc:?}").into_bytes()
}
