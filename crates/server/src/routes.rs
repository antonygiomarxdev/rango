use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    Extension, extract::Json,
    http::{HeaderValue, StatusCode},
};
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
    RangoError, RetrievalCapabilityRequest, RetrievalCapabilityResponse, RetrievalStatus,
};
use tracing::{info, instrument};

use crate::retrieval::{RetrievalRuntime, ranking::rank_candidates_v1};

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
    containment: Mutex<HashMap<(String, String), ContainmentState>>,
    audit_counter: AtomicU64,
    retrieval_runtime: RetrievalRuntime,
}

impl ServerState {
    pub fn new(oplog: Arc<dyn Oplog>) -> Self {
        Self::with_control_plane(oplog, Arc::new(ControlPlane::default()))
    }

    pub fn with_control_plane(oplog: Arc<dyn Oplog>, control_plane: Arc<ControlPlane>) -> Self {
        Self {
            oplog,
            tokens: Mutex::new(HashMap::new()),
            non_owner_rejections: AtomicU64::new(0),
            cross_tenant_rejections: AtomicU64::new(0),
            control_plane,
            containment: Mutex::new(HashMap::new()),
            audit_counter: AtomicU64::new(0),
            retrieval_runtime: RetrievalRuntime::fallback_only(),
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

    fn containment_gate(&self, tenant_id: &str, namespace: &str) -> Option<GovernanceDecision> {
        let key = (tenant_id.to_string(), namespace.to_string());
        let mut map = self.containment.lock().unwrap();
        let state = map.entry(key).or_default();
        state.maybe_reset_after_cooldown();
        match state.mode {
            ContainmentMode::Normal => None,
            ContainmentMode::Throttle => Some(GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "containment_throttle".to_string(),
            }),
            ContainmentMode::Reject => Some(GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "containment_reject".to_string(),
            }),
        }
    }

    fn register_decision_outcome(&self, tenant_id: &str, namespace: &str, decision: &GovernanceDecision) {
        let key = (tenant_id.to_string(), namespace.to_string());
        let mut map = self.containment.lock().unwrap();
        let state = map.entry(key).or_default();
        state.observe_decision(decision);
    }

    fn persist_audit_evidence(
        &self,
        stage: &str,
        tenant_id: &str,
        namespace: &str,
        write_id: Option<&str>,
        decision: &GovernanceDecision,
    ) -> Result<u64, RangoError> {
        let doc_id = rango_types::DocumentId::new_uuid_v7();
        let rev = rango_types::Revision::now("governance-runtime");
        let mut patch = Document::new();
        patch.insert("stage", stage.to_string());
        patch.insert("decision", ControlPlane::decision_label(decision).to_string());
        patch.insert("reason", decision.reason.clone());
        patch.insert("tenant_id", tenant_id.to_string());
        patch.insert("namespace", namespace.to_string());
        patch.insert("write_id", write_id.unwrap_or("none").to_string());
        patch.insert("recorded_at", bson::DateTime::now());

        let event_num = self.audit_counter.fetch_add(1, Ordering::Relaxed) + 1;
        let audit_write_id = format!(
            "governance-audit:{tenant_id}:{namespace}:{stage}:{}:{event_num}",
            write_id.unwrap_or("none")
        );
        let mutation = Mutation {
            op: rango_types::MutationOp::Insert,
            collection: "__governance_audit".to_string(),
            doc_id: doc_id.clone(),
            patch: Some(patch),
            seq: 0,
            timestamp: bson::DateTime::now(),
            rev: rev.clone(),
            write_id: audit_write_id,
            metadata: rango_types::MutationMetadata {
                id: doc_id.clone(),
                namespace: namespace.to_string(),
                tenant_id: tenant_id.to_string(),
                r#type: "governance_audit".to_string(),
                rev,
                created_at: bson::DateTime::now(),
                updated_at: bson::DateTime::now(),
                source: "server.runtime".to_string(),
                actor: "governance".to_string(),
                lineage: doc_id.to_string(),
                schema_version: 1,
                trust_score: 1.0,
                verified: Some(true),
                expires_at: None,
            },
        };
        let entry = OplogEntry {
            seq: 0,
            timestamp: bson::DateTime::now(),
            mutation,
            origin: OplogOrigin::Local,
            applied: true,
            snapshot_anchor: None,
        };
        self.oplog.append(entry)
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum ContainmentMode {
    #[default]
    Normal,
    Throttle,
    Reject,
}

#[derive(Debug)]
struct ContainmentState {
    mode: ContainmentMode,
    reject_count: u32,
    last_event: Instant,
}

impl Default for ContainmentState {
    fn default() -> Self {
        Self {
            mode: ContainmentMode::Normal,
            reject_count: 0,
            last_event: Instant::now(),
        }
    }
}

impl ContainmentState {
    const THROTTLE_THRESHOLD: u32 = 3;
    const REJECT_THRESHOLD: u32 = 5;
    const COOLDOWN: Duration = Duration::from_millis(500);

    fn maybe_reset_after_cooldown(&mut self) {
        if self.last_event.elapsed() >= Self::COOLDOWN {
            self.mode = ContainmentMode::Normal;
            self.reject_count = 0;
        }
    }

    fn observe_decision(&mut self, decision: &GovernanceDecision) {
        self.last_event = Instant::now();
        if matches!(decision.decision, PolicyDecision::Reject) {
            self.reject_count += 1;
            if self.reject_count >= Self::REJECT_THRESHOLD {
                self.mode = ContainmentMode::Reject;
            } else if self.reject_count >= Self::THROTTLE_THRESHOLD {
                self.mode = ContainmentMode::Throttle;
            }
            return;
        }

        self.mode = ContainmentMode::Normal;
        self.reject_count = 0;
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
        let decision = GovernanceDecision {
            decision: PolicyDecision::Reject,
            reason: "tenant_mismatch".to_string(),
        };
        let _ = state.persist_audit_evidence(
            "write",
            &req.tenant_id,
            &req.namespace,
            None,
            &decision,
        );
        state.register_decision_outcome(&req.tenant_id, &req.namespace, &decision);
        let new_checkpoint = Checkpoint(state.oplog.latest_seq().unwrap_or(0));
        return Ok(Json(PushResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint,
            rejected_non_owner_count: 0,
            rejected_cross_tenant_count: 1,
            audit: vec![decision],
        }));
    }

    let mut accepted_seqs = Vec::new();
    let mut rejected_cross_tenant_count = 0u64;
    let mut audit = Vec::new();
    for mutation in req.mutations {
        if let Some(containment) = state.containment_gate(&req.tenant_id, &req.namespace) {
            let _ = state.persist_audit_evidence(
                "write",
                &req.tenant_id,
                &req.namespace,
                Some(&mutation.write_id),
                &containment,
            );
            state.register_decision_outcome(&req.tenant_id, &req.namespace, &containment);
            audit.push(containment);
            continue;
        }

        if let Err(err) = mutation.validate_metadata() {
            let decision = GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: format!("invalid_metadata:{err}"),
            };
            let _ = state.persist_audit_evidence(
                "write",
                &req.tenant_id,
                &req.namespace,
                Some(&mutation.write_id),
                &decision,
            );
            state.register_decision_outcome(&req.tenant_id, &req.namespace, &decision);
            audit.push(decision);
            continue;
        }

        if mutation.metadata.tenant_id != req.tenant_id
            || mutation.metadata.namespace != req.namespace
        {
            state
                .cross_tenant_rejections
                .fetch_add(1, Ordering::Relaxed);
            rejected_cross_tenant_count += 1;
            let decision = GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: "cross_tenant_or_namespace_mutation".to_string(),
            };
            let _ = state.persist_audit_evidence(
                "write",
                &req.tenant_id,
                &req.namespace,
                Some(&mutation.write_id),
                &decision,
            );
            state.register_decision_outcome(&req.tenant_id, &req.namespace, &decision);
            audit.push(decision);
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
        let decision = normalize_write_decision(decision);
        let _ = state.persist_audit_evidence(
            "write",
            &req.tenant_id,
            &req.namespace,
            Some(&mutation.write_id),
            &decision,
        );
        state.register_decision_outcome(&req.tenant_id, &req.namespace, &decision);

        if ControlPlane::is_reject(&decision) {
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

    let read_tier = parse_read_tier(headers.get("X-Rango-Read-Tier"));
    let scoped_entries: Vec<OplogEntry> = entries
        .into_iter()
        .filter(|e| {
            e.mutation.metadata.tenant_id == req.tenant_id
                && e.mutation.metadata.namespace == req.namespace
                && e.mutation.metadata.r#type != "governance_audit"
                && mutation_visible_for_tier(&e.mutation, read_tier)
        })
        .collect();

    let candidates: Vec<Document> = scoped_entries
        .iter()
        .filter_map(add_candidate_identity)
        .collect();
    let read_request = ReadRequest {
        tenant_id: req.tenant_id.clone(),
        namespace: req.namespace.clone(),
        tier: read_tier,
        limit: 1000,
    };
    let (read_decision, filtered_candidates) = state
        .control_plane
        .read_path(&read_request, candidates)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let read_decision = state
        .containment_gate(&req.tenant_id, &req.namespace)
        .unwrap_or(read_decision);
    let _ = state.persist_audit_evidence(
        "read",
        &req.tenant_id,
        &req.namespace,
        None,
        &read_decision,
    );
    state.register_decision_outcome(&req.tenant_id, &req.namespace, &read_decision);
    if ControlPlane::is_reject(&read_decision) {
        return Ok(Json(PullResponse {
            mutations: Vec::new(),
            new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(0)),
            audit: vec![read_decision],
        }));
    }

    let mut allowed_identity_counts: HashMap<(u64, String), usize> = HashMap::new();
    for candidate in &filtered_candidates {
        let Some(key) = candidate_identity(candidate) else {
            continue;
        };
        *allowed_identity_counts.entry(key).or_insert(0) += 1;
    }

    let mut mutations: Vec<Mutation> = Vec::new();
    for entry in scoped_entries {
        if entry.mutation.patch.is_none() {
            continue;
        }
        let key = (entry.seq, entry.mutation.write_id.clone());
        let Some(remaining) = allowed_identity_counts.get_mut(&key) else {
            continue;
        };
        if *remaining > 0 {
            let mutation = annotate_mutation_for_read_tier(entry.mutation, read_tier);
            mutations.push(mutation);
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
pub async fn handle_retrieval_read(
    Extension(state): Extension<Arc<ServerState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<RetrievalCapabilityRequest>,
) -> Result<Json<RetrievalCapabilityResponse>, StatusCode> {
    let protocol_version = headers
        .get("X-Rango-Protocol-Version")
        .and_then(|v| v.to_str().ok());
    if protocol_version != Some("1") {
        return Err(StatusCode::BAD_REQUEST);
    }

    let auth = headers.get("Authorization").and_then(|v| v.to_str().ok());
    let principal = state.validate_token(auth).ok_or(StatusCode::UNAUTHORIZED)?;
    if principal.tenant_id != req.tenant_id {
        return Err(StatusCode::FORBIDDEN);
    }

    let request = ReadRequest {
        tenant_id: req.tenant_id.clone(),
        namespace: req.namespace.clone(),
        tier: MemoryTier::State,
        limit: req.limit,
    };

    let response = match state.retrieval_runtime.retrieve(&req) {
        Ok(candidates) => {
            let ranked = rank_candidates_v1(candidates);
            let docs: Vec<Document> = ranked.iter().map(|candidate| candidate.payload.clone()).collect();
            let (decision, filtered) = state
                .control_plane
                .read_path(&request, docs)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let filtered_ids: std::collections::HashSet<String> = filtered
                .iter()
                .filter_map(|doc| doc.get_str("candidate_id").ok().map(ToString::to_string))
                .collect();
            let bounded_ranked = ranked
                .into_iter()
                .filter(|candidate| filtered_ids.contains(&candidate.candidate_id))
                .take(req.limit)
                .collect::<Vec<_>>();

            state.register_decision_outcome(&req.tenant_id, &req.namespace, &decision);
            let _ = state.persist_audit_evidence(
                "retrieval",
                &req.tenant_id,
                &req.namespace,
                None,
                &decision,
            );
            RetrievalCapabilityResponse {
                status: RetrievalStatus::Healthy,
                retrieval_status_reason: decision.reason,
                canonical_fallback: false,
                candidates: bounded_ranked,
            }
        }
        Err(err) => {
            let decision = GovernanceDecision {
                decision: PolicyDecision::Sanitize,
                reason: err.reason.clone(),
            };
            state.register_decision_outcome(&req.tenant_id, &req.namespace, &decision);
            let _ = state.persist_audit_evidence(
                "retrieval",
                &req.tenant_id,
                &req.namespace,
                None,
                &decision,
            );
            RetrievalCapabilityResponse {
                status: RetrievalStatus::Degraded,
                retrieval_status_reason: err.reason,
                canonical_fallback: true,
                candidates: Vec::new(),
            }
        }
    };

    Ok(Json(response))
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
    let semantic_path_decision = ControlPlane::validate_semantic_promotion(&promotion_request);
    if ControlPlane::is_reject(&semantic_path_decision) {
        let _ = state.persist_audit_evidence(
            "promotion",
            &req.tenant_id,
            &req.namespace,
            Some(&req.mutation.write_id),
            &semantic_path_decision,
        );
        state.register_decision_outcome(&req.tenant_id, &req.namespace, &semantic_path_decision);
        return Ok(Json(PromoteResponse {
            accepted_seqs: Vec::new(),
            new_checkpoint: Checkpoint(state.oplog.latest_seq().unwrap_or(0)),
            rejected_count: 1,
            audit: vec![semantic_path_decision],
        }));
    }

    let payload = WritePayload::StateWithTrust {
        document: req.mutation.patch.clone().unwrap_or_else(Document::new),
        trust_score: req.mutation.metadata.trust_score,
    };

    let (decision, _) = state
        .control_plane
        .promotion_path(&promotion_request, &payload)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let decision = state
        .containment_gate(&req.tenant_id, &req.namespace)
        .unwrap_or(decision);
    let _ = state.persist_audit_evidence(
        "promotion",
        &req.tenant_id,
        &req.namespace,
        Some(&req.mutation.write_id),
        &decision,
    );
    state.register_decision_outcome(&req.tenant_id, &req.namespace, &decision);

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

const IDENTITY_SEQ_FIELD: &str = "__rango_candidate_seq";
const IDENTITY_WRITE_ID_FIELD: &str = "__rango_candidate_write_id";

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

fn add_candidate_identity(entry: &OplogEntry) -> Option<Document> {
    let mut candidate = entry.mutation.patch.clone()?;
    candidate.insert(IDENTITY_SEQ_FIELD, entry.seq as i64);
    candidate.insert(IDENTITY_WRITE_ID_FIELD, entry.mutation.write_id.clone());
    Some(candidate)
}

fn candidate_identity(candidate: &Document) -> Option<(u64, String)> {
    let seq = candidate.get_i64(IDENTITY_SEQ_FIELD).ok()? as u64;
    let write_id = candidate.get_str(IDENTITY_WRITE_ID_FIELD).ok()?.to_string();
    Some((seq, write_id))
}

fn normalize_write_decision(mut decision: GovernanceDecision) -> GovernanceDecision {
    if matches!(decision.decision, PolicyDecision::Reject)
        && decision.reason.starts_with("trust_score_below_threshold")
    {
        decision.reason = format!("poisoning_low_trust:{}", decision.reason);
    }
    decision
}

fn parse_read_tier(header: Option<&HeaderValue>) -> MemoryTier {
    match header.and_then(|v| v.to_str().ok()) {
        Some("semantic") => MemoryTier::Semantic,
        _ => MemoryTier::State,
    }
}

fn mutation_visible_for_tier(mutation: &Mutation, tier: MemoryTier) -> bool {
    match tier {
        MemoryTier::Semantic => mutation.metadata.r#type == "semantic_projection",
        _ => mutation.metadata.r#type != "semantic_projection",
    }
}

fn annotate_mutation_for_read_tier(mut mutation: Mutation, tier: MemoryTier) -> Mutation {
    if tier == MemoryTier::Semantic {
        if let Some(patch) = mutation.patch.as_mut() {
            patch.insert("derived", true);
            patch.insert("canonical", false);
        }
    }
    mutation
}
