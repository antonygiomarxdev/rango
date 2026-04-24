pub mod adapters;
pub mod ranking;

use std::sync::Arc;

use adapters::{AdapterError, AdapterErrorKind, GraphRetrievalAdapter, VectorRetrievalAdapter};
use rango_types::{RetrievalCandidate, RetrievalCapabilityRequest};

#[derive(Clone)]
pub struct RetrievalRuntime {
    vector: Arc<dyn VectorRetrievalAdapter>,
    graph: Arc<dyn GraphRetrievalAdapter>,
}

impl RetrievalRuntime {
    pub fn new(
        vector: Arc<dyn VectorRetrievalAdapter>,
        graph: Arc<dyn GraphRetrievalAdapter>,
    ) -> Self {
        Self { vector, graph }
    }

    pub fn fallback_only() -> Self {
        Self::new(
            Arc::new(adapters::AdapterCapabilities),
            Arc::new(adapters::AdapterCapabilities),
        )
    }

    pub fn retrieve(
        &self,
        request: &RetrievalCapabilityRequest,
    ) -> Result<Vec<RetrievalCandidate>, AdapterError> {
        let mut candidates = Vec::new();
        let vector = self.vector.query_vector(request);
        let graph = self.graph.query_graph(request);

        match vector {
            Ok(items) => candidates.extend(items),
            Err(err) => {
                if !matches!(
                    err.kind,
                    AdapterErrorKind::Unavailable | AdapterErrorKind::Timeout
                ) {
                    return Err(err);
                }
            }
        }

        match graph {
            Ok(items) => candidates.extend(items),
            Err(err) => {
                if !matches!(
                    err.kind,
                    AdapterErrorKind::Unavailable | AdapterErrorKind::Timeout
                ) {
                    return Err(err);
                }
            }
        }

        if candidates.is_empty() {
            return Err(AdapterError::unavailable("adapter_unavailable"));
        }

        Ok(candidates)
    }
}
