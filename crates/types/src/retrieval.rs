use bson::Document;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalSource {
    Canonical,
    Vector,
    Graph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetrievalStatus {
    Healthy,
    Disabled,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalCapabilityRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub query: String,
    pub limit: usize,
    pub vector_limit: usize,
    pub graph_limit: usize,
}

impl RetrievalCapabilityRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        namespace: impl Into<String>,
        query: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            namespace: namespace.into(),
            query: query.into(),
            limit: 20,
            vector_limit: 20,
            graph_limit: 20,
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankingSignals {
    pub relevance: f64,
    pub trust: f64,
    pub recency: f64,
    pub provenance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankingExplainability {
    pub formula_version: String,
    pub relevance_weight: f64,
    pub trust_weight: f64,
    pub recency_weight: f64,
    pub provenance_weight: f64,
    pub relevance_component: f64,
    pub trust_component: f64,
    pub recency_component: f64,
    pub provenance_component: f64,
    pub total_score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalCandidate {
    pub candidate_id: String,
    pub tenant_id: String,
    pub namespace: String,
    pub source: RetrievalSource,
    pub lineage: String,
    pub timestamp_ms: i64,
    pub payload: Document,
    pub signals: RankingSignals,
    pub score: f64,
    pub explainability: Option<RankingExplainability>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrievalCapabilityResponse {
    pub status: RetrievalStatus,
    pub retrieval_status_reason: String,
    pub canonical_fallback: bool,
    pub candidates: Vec<RetrievalCandidate>,
}

pub const RANKING_FORMULA_V1: &str = "v1";

pub fn deterministic_score_v1(signals: &RankingSignals) -> f64 {
    (0.35 * signals.relevance)
        + (0.30 * signals.trust)
        + (0.20 * signals.recency)
        + (0.15 * signals.provenance)
}
