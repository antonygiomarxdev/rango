use serde::{Deserialize, Serialize};
use bson::Document;

/// Canonical substrate memory tiers governed by the control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTier {
    State,
    Episodic,
    Semantic,
    Artifact,
}

/// Trust classification for policy decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Low,
    Medium,
    High,
}

/// Result emitted by policy hooks for auditable control-plane decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Sanitize,
    Reject,
}

/// Auditable outcome for control-plane operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceDecision {
    pub decision: PolicyDecision,
    pub reason: String,
}

/// Read payload marker for derived/non-canonical semantic views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticReadView {
    pub tier: MemoryTier,
    pub derived: bool,
    pub canonical: bool,
    pub payload: Document,
}

impl SemanticReadView {
    pub fn new(tier: MemoryTier, payload: Document) -> Self {
        Self {
            tier,
            derived: true,
            canonical: false,
            payload,
        }
    }
}
