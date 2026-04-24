use rango_sdk::{TieredMemoryReadRequest, TieredMemoryWriteRequest};
use rango_types::MemoryTier;

#[test]
fn tiered_memory_api_is_generic_and_typed() {
    let read_req = TieredMemoryReadRequest::new("tenant-a", "ns-a", MemoryTier::Semantic)
        .with_limit(25)
        .require_derived_label(true);
    assert_eq!(read_req.tenant_id, "tenant-a");
    assert_eq!(read_req.namespace, "ns-a");
    assert_eq!(read_req.tier, MemoryTier::Semantic);
    assert_eq!(read_req.limit, 25);
    assert!(read_req.require_derived_label);

    let write_req = TieredMemoryWriteRequest::new("tenant-a", "ns-a", MemoryTier::Artifact);
    assert_eq!(write_req.tenant_id, "tenant-a");
    assert_eq!(write_req.namespace, "ns-a");
    assert_eq!(write_req.tier, MemoryTier::Artifact);
}
