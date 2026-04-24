pub mod client;
pub mod migrate;

use bson::Document;
use rango_types::MemoryTier;

pub use client::*;
pub use migrate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedReadLabel {
    pub derived: bool,
    pub canonical: bool,
}

impl DerivedReadLabel {
    pub fn derived_non_canonical() -> Self {
        Self {
            derived: true,
            canonical: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SemanticProjectionRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub candidate_id: String,
    pub tier: MemoryTier,
    pub payload: Document,
}

impl SemanticProjectionRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        namespace: impl Into<String>,
        candidate_id: impl Into<String>,
        payload: Document,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            namespace: namespace.into(),
            candidate_id: candidate_id.into(),
            tier: MemoryTier::Semantic,
            payload,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SemanticProjectionResponse {
    pub accepted: bool,
    pub write_id: String,
    pub label: DerivedReadLabel,
}

#[derive(Debug, Clone)]
pub struct TieredMemoryReadRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub tier: MemoryTier,
    pub limit: usize,
    pub require_derived_label: bool,
}

impl TieredMemoryReadRequest {
    pub fn new(
        tenant_id: impl Into<String>,
        namespace: impl Into<String>,
        tier: MemoryTier,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            namespace: namespace.into(),
            tier,
            limit: 100,
            require_derived_label: false,
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn require_derived_label(mut self, required: bool) -> Self {
        self.require_derived_label = required;
        self
    }
}

#[derive(Debug, Clone)]
pub struct TieredMemoryWriteRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub tier: MemoryTier,
}

impl TieredMemoryWriteRequest {
    pub fn new(tenant_id: impl Into<String>, namespace: impl Into<String>, tier: MemoryTier) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            namespace: namespace.into(),
            tier,
        }
    }
}
