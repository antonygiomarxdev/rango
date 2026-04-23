use std::sync::Arc;

use bson::Document;
use rango_types::{
    ArtifactEnvelope, EventEnvelope, GovernanceDecision, MemoryTier, PolicyDecision, RangoError,
};

#[derive(Debug, Clone)]
pub struct WriteContext {
    pub tenant_id: String,
    pub namespace: String,
    pub actor: String,
    pub source: String,
    pub tier: MemoryTier,
}

#[derive(Debug, Clone)]
pub struct ReadRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub tier: MemoryTier,
    pub limit: usize,
}

#[derive(Debug, Clone)]
pub struct PromotionRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub from: MemoryTier,
    pub to: MemoryTier,
    pub candidate_id: String,
}

#[derive(Debug, Clone)]
pub enum WritePayload {
    State(Document),
    StateWithTrust {
        document: Document,
        trust_score: f64,
    },
    Event(EventEnvelope),
    Artifact(ArtifactEnvelope),
    Semantic(Document),
}

pub trait WriteValidationHook: Send + Sync {
    fn validate(&self, ctx: &WriteContext, payload: &WritePayload) -> GovernanceDecision;
}

pub trait TrustScoringHook: Send + Sync {
    fn score(&self, ctx: &WriteContext, payload: &WritePayload) -> f64;
}

pub trait PromotionGateHook: Send + Sync {
    fn sanitize(&self, request: &PromotionRequest, payload: &WritePayload) -> WritePayload;
    fn allow(&self, request: &PromotionRequest, payload: &WritePayload) -> GovernanceDecision;
}

pub trait RetrievalGateHook: Send + Sync {
    fn allow(&self, request: &ReadRequest) -> GovernanceDecision;
}

pub trait BoundedContextFilterHook: Send + Sync {
    fn apply(&self, request: &ReadRequest, candidates: Vec<Document>) -> Vec<Document>;
}

pub trait AnomalySignalHook: Send + Sync {
    fn evaluate(&self, stage: &'static str, decision: &GovernanceDecision);
}

pub trait AuditSink: Send + Sync {
    fn record(&self, stage: &'static str, decision: &GovernanceDecision);
}

pub struct NoopWriteValidationHook;

impl WriteValidationHook for NoopWriteValidationHook {
    fn validate(&self, _ctx: &WriteContext, _payload: &WritePayload) -> GovernanceDecision {
        GovernanceDecision {
            decision: PolicyDecision::Allow,
            reason: "validation_pass".to_string(),
        }
    }
}

pub struct NoopTrustScoringHook;

impl TrustScoringHook for NoopTrustScoringHook {
    fn score(&self, _ctx: &WriteContext, payload: &WritePayload) -> f64 {
        match payload {
            WritePayload::StateWithTrust { trust_score, .. } => *trust_score,
            _ => 1.0,
        }
    }
}

pub struct NoopPromotionGateHook;

impl PromotionGateHook for NoopPromotionGateHook {
    fn sanitize(&self, _request: &PromotionRequest, payload: &WritePayload) -> WritePayload {
        payload.clone()
    }

    fn allow(&self, _request: &PromotionRequest, _payload: &WritePayload) -> GovernanceDecision {
        GovernanceDecision {
            decision: PolicyDecision::Allow,
            reason: "promotion_allowed".to_string(),
        }
    }
}

pub struct NoopRetrievalGateHook;

impl RetrievalGateHook for NoopRetrievalGateHook {
    fn allow(&self, _request: &ReadRequest) -> GovernanceDecision {
        GovernanceDecision {
            decision: PolicyDecision::Allow,
            reason: "retrieval_allowed".to_string(),
        }
    }
}

pub struct NoopBoundedContextFilterHook;

impl BoundedContextFilterHook for NoopBoundedContextFilterHook {
    fn apply(&self, _request: &ReadRequest, candidates: Vec<Document>) -> Vec<Document> {
        candidates
    }
}

pub struct NoopAnomalySignalHook;

impl AnomalySignalHook for NoopAnomalySignalHook {
    fn evaluate(&self, _stage: &'static str, _decision: &GovernanceDecision) {}
}

pub struct NoopAuditSink;

impl AuditSink for NoopAuditSink {
    fn record(&self, _stage: &'static str, _decision: &GovernanceDecision) {}
}

pub struct ControlPlane {
    validation_hook: Arc<dyn WriteValidationHook>,
    trust_hook: Arc<dyn TrustScoringHook>,
    promotion_hook: Arc<dyn PromotionGateHook>,
    retrieval_hook: Arc<dyn RetrievalGateHook>,
    bounded_context_hook: Arc<dyn BoundedContextFilterHook>,
    anomaly_hook: Arc<dyn AnomalySignalHook>,
    audit_sink: Arc<dyn AuditSink>,
}

impl Default for ControlPlane {
    fn default() -> Self {
        Self {
            validation_hook: Arc::new(NoopWriteValidationHook),
            trust_hook: Arc::new(NoopTrustScoringHook),
            promotion_hook: Arc::new(NoopPromotionGateHook),
            retrieval_hook: Arc::new(NoopRetrievalGateHook),
            bounded_context_hook: Arc::new(NoopBoundedContextFilterHook),
            anomaly_hook: Arc::new(NoopAnomalySignalHook),
            audit_sink: Arc::new(NoopAuditSink),
        }
    }
}

impl ControlPlane {
    pub fn is_reject(decision: &GovernanceDecision) -> bool {
        matches!(decision.decision, PolicyDecision::Reject)
    }

    pub fn decision_label(decision: &GovernanceDecision) -> &'static str {
        match decision.decision {
            PolicyDecision::Allow => "allow",
            PolicyDecision::Sanitize => "sanitize",
            PolicyDecision::Reject => "reject",
        }
    }

    pub fn with_hooks(
        validation_hook: Arc<dyn WriteValidationHook>,
        trust_hook: Arc<dyn TrustScoringHook>,
        promotion_hook: Arc<dyn PromotionGateHook>,
        retrieval_hook: Arc<dyn RetrievalGateHook>,
        bounded_context_hook: Arc<dyn BoundedContextFilterHook>,
        anomaly_hook: Arc<dyn AnomalySignalHook>,
        audit_sink: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            validation_hook,
            trust_hook,
            promotion_hook,
            retrieval_hook,
            bounded_context_hook,
            anomaly_hook,
            audit_sink,
        }
    }

    /// Deterministic write hook order:
    /// 1) write validation, 2) trust scoring, 3) audit/anomaly signaling.
    pub fn write_path(
        &self,
        ctx: &WriteContext,
        payload: &WritePayload,
    ) -> Result<GovernanceDecision, RangoError> {
        let validation = self.validation_hook.validate(ctx, payload);
        self.audit_sink.record("write.validate", &validation);
        self.anomaly_hook.evaluate("write.validate", &validation);

        if matches!(validation.decision, PolicyDecision::Reject) {
            return Ok(validation);
        }

        let trust = self.trust_hook.score(ctx, payload);
        let decision = if trust < 0.25 {
            GovernanceDecision {
                decision: PolicyDecision::Reject,
                reason: format!("trust_score_below_threshold:{trust:.2}"),
            }
        } else {
            GovernanceDecision {
                decision: PolicyDecision::Allow,
                reason: format!("trust_score:{trust:.2}"),
            }
        };

        self.audit_sink.record("write.trust", &decision);
        self.anomaly_hook.evaluate("write.trust", &decision);
        Ok(decision)
    }

    /// Deterministic read hook order:
    /// 1) retrieval gate, 2) read audit, 3) read anomaly, 4) bounded-context filter.
    pub fn read_path(
        &self,
        request: &ReadRequest,
        candidates: Vec<Document>,
    ) -> Result<(GovernanceDecision, Vec<Document>), RangoError> {
        let retrieval = self.retrieval_hook.allow(request);
        self.audit_sink.record("read.gate", &retrieval);
        self.anomaly_hook.evaluate("read.gate", &retrieval);

        if matches!(retrieval.decision, PolicyDecision::Reject) {
            return Ok((retrieval, Vec::new()));
        }

        let filtered = self.bounded_context_hook.apply(request, candidates);
        Ok((retrieval, filtered))
    }

    /// Deterministic promotion hook order:
    /// 1) sanitize candidate, 2) promotion gate decision, 3) audit/anomaly signaling.
    pub fn promotion_path(
        &self,
        request: &PromotionRequest,
        payload: &WritePayload,
    ) -> Result<(GovernanceDecision, WritePayload), RangoError> {
        let sanitized = self.promotion_hook.sanitize(request, payload);
        let decision = self.promotion_hook.allow(request, &sanitized);

        self.audit_sink.record("promotion.gate", &decision);
        self.anomaly_hook.evaluate("promotion.gate", &decision);

        Ok((decision, sanitized))
    }
}
