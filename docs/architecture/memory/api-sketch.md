# Memory Control Plane API Sketch

## Core Boundary

Control-plane contracts are substrate-level and live in `rango_core::control_plane`.
No workflow/product semantics are part of these interfaces.

## Typed Paths

```rust
pub struct ControlPlane;

impl ControlPlane {
    pub fn write_path(
        &self,
        ctx: &WriteContext,
        payload: &WritePayload,
    ) -> Result<GovernanceDecision, RangoError>;

    pub fn read_path(
        &self,
        request: &ReadRequest,
        candidates: Vec<bson::Document>,
    ) -> Result<(GovernanceDecision, Vec<bson::Document>), RangoError>;

    pub fn promotion_path(
        &self,
        request: &PromotionRequest,
        payload: &WritePayload,
    ) -> Result<(GovernanceDecision, WritePayload), RangoError>;
}
```

## Tier Routing

```rust
pub enum MemoryTier {
    State,
    Episodic,
    Semantic,
    Artifact,
}

pub enum WritePayload {
    State(bson::Document),
    Event(EventEnvelope),
    Artifact(ArtifactEnvelope),
    Semantic(bson::Document),
}
```

`promotion_path` is explicit and policy-gated for transitions such as `episodic -> semantic`.
Promotion is never implicit in CRUD operations.
Server runtime exposes this path at `POST /promote` and enforces it before append.

## Policy Hooks

```rust
pub trait WriteValidationHook {
    fn validate(&self, ctx: &WriteContext, payload: &WritePayload) -> GovernanceDecision;
}

pub trait TrustScoringHook {
    fn score(&self, ctx: &WriteContext, payload: &WritePayload) -> f64;
}

pub trait PromotionGateHook {
    fn sanitize(&self, request: &PromotionRequest, payload: &WritePayload) -> WritePayload;
    fn allow(&self, request: &PromotionRequest, payload: &WritePayload) -> GovernanceDecision;
}

pub trait RetrievalGateHook {
    fn allow(&self, request: &ReadRequest) -> GovernanceDecision;
}

pub trait BoundedContextFilterHook {
    fn apply(&self, request: &ReadRequest, candidates: Vec<bson::Document>) -> Vec<bson::Document>;
}

pub trait AnomalySignalHook {
    fn evaluate(&self, stage: &'static str, decision: &GovernanceDecision);
}

pub trait AuditSink {
    fn record(&self, stage: &'static str, decision: &GovernanceDecision);
}
```

## Deterministic Hook Order

- `write_path`: `validate -> trust score -> audit/anomaly`
- `read_path`: `retrieval gate -> audit -> anomaly -> bounded-context filter`
- `promotion_path`: `sanitize -> gate -> audit/anomaly`

## Baseline Implementation

`ControlPlane::default()` wires no-op hooks with deterministic ordering so integration points are compile-safe before policy specialization.
