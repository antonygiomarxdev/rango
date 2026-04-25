pub mod client;
pub mod migrate;

pub use bson::Document;
use rango_types::{MemoryTier, RangoDocument, RangoError};

pub use client::*;
pub use migrate::*;
pub use rango_types::{
    RankingExplainability, RankingSignals, RetrievalCandidate, RetrievalCapabilityRequest,
    RetrievalCapabilityResponse, RetrievalSource, RetrievalStatus,
};

/// Stable wrapper around rango_core::Cursor for public SDK use.
/// Provides a read-only cursor interface for iterating over documents.
pub struct Cursor(pub(crate) rango_core::Cursor);

impl Cursor {
    /// Consume the cursor and call the provided closure for each document.
    pub fn for_each<F>(self, mut f: F) -> Result<(), RangoError>
    where
        F: FnMut(RangoDocument) -> Result<(), RangoError>,
    {
        for result in self {
            let doc = result?;
            f(doc)?;
        }
        Ok(())
    }
}

impl Iterator for Cursor {
    type Item = Result<RangoDocument, RangoError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl From<rango_core::Cursor> for Cursor {
    #[doc(hidden)]
    fn from(inner: rango_core::Cursor) -> Self {
        Cursor(inner)
    }
}

/// Experimental: Internal metadata for read classification.
/// This type is unstable and may change without notice.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedReadLabel {
    pub derived: bool,
    pub canonical: bool,
}

#[doc(hidden)]
impl DerivedReadLabel {
    #[doc(hidden)]
    pub fn derived_non_canonical() -> Self {
        Self {
            derived: true,
            canonical: false,
        }
    }
}

/// Experimental: Semantic projection request.
/// This type is unstable and may change without notice.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct SemanticProjectionRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub candidate_id: String,
    pub tier: MemoryTier,
    pub payload: Document,
}

#[doc(hidden)]
impl SemanticProjectionRequest {
    #[doc(hidden)]
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

/// Experimental: Semantic projection response.
/// This type is unstable and may change without notice.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct SemanticProjectionResponse {
    pub accepted: bool,
    pub write_id: String,
    pub label: DerivedReadLabel,
}

/// Experimental: Tiered memory read request.
/// This type is unstable and may change without notice.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct TieredMemoryReadRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub tier: MemoryTier,
    pub limit: usize,
    pub require_derived_label: bool,
}

#[doc(hidden)]
impl TieredMemoryReadRequest {
    #[doc(hidden)]
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

    #[doc(hidden)]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    #[doc(hidden)]
    pub fn require_derived_label(mut self, required: bool) -> Self {
        self.require_derived_label = required;
        self
    }
}

/// Experimental: Tiered memory write request.
/// This type is unstable and may change without notice.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct TieredMemoryWriteRequest {
    pub tenant_id: String,
    pub namespace: String,
    pub tier: MemoryTier,
}

#[doc(hidden)]
impl TieredMemoryWriteRequest {
    #[doc(hidden)]
    pub fn new(
        tenant_id: impl Into<String>,
        namespace: impl Into<String>,
        tier: MemoryTier,
    ) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            namespace: namespace.into(),
            tier,
        }
    }
}
