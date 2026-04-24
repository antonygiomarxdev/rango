use std::sync::Arc;

use bson::{doc, Document};
use rango_core::{
    BoundedContextFilterHook, ControlPlane, NoopAnomalySignalHook, NoopAuditSink, NoopPromotionGateHook,
    NoopRetrievalGateHook, NoopTrustScoringHook, NoopWriteValidationHook, ReadRequest,
};
use rango_types::MemoryTier;

struct TenantNamespaceBoundFilter;

impl BoundedContextFilterHook for TenantNamespaceBoundFilter {
    fn apply(&self, request: &ReadRequest, candidates: Vec<Document>) -> Vec<Document> {
        candidates
            .into_iter()
            .filter(|candidate| {
                candidate.get_str("tenant_id").ok() == Some(request.tenant_id.as_str())
                    && candidate.get_str("namespace").ok() == Some(request.namespace.as_str())
            })
            .collect()
    }
}

#[test]
fn bounded_context_filter_runs_after_retrieval_gate_and_enforces_scope() {
    let control_plane = ControlPlane::with_hooks(
        Arc::new(NoopWriteValidationHook),
        Arc::new(NoopTrustScoringHook),
        Arc::new(NoopPromotionGateHook),
        Arc::new(NoopRetrievalGateHook),
        Arc::new(TenantNamespaceBoundFilter),
        Arc::new(NoopAnomalySignalHook),
        Arc::new(NoopAuditSink),
    );
    let request = ReadRequest {
        tenant_id: "tenant-a".to_string(),
        namespace: "ns-a".to_string(),
        tier: MemoryTier::State,
        limit: 8,
    };
    let candidates = vec![
        doc! { "tenant_id": "tenant-a", "namespace": "ns-a", "id": "ok-1" },
        doc! { "tenant_id": "tenant-b", "namespace": "ns-a", "id": "drop-tenant" },
        doc! { "tenant_id": "tenant-a", "namespace": "ns-b", "id": "drop-namespace" },
    ];

    let (decision, filtered) = control_plane.read_path(&request, candidates).unwrap();
    assert_eq!(decision.reason, "retrieval_allowed");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].get_str("id").unwrap(), "ok-1");
}
