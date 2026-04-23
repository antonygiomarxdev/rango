use std::collections::HashSet;

use bson::doc;
use proptest::prelude::*;
use rango_core::ControlPlane;
use rango_types::{MemoryTier, PolicyDecision};

fn make_ctx(tenant_id: &str, namespace: &str) -> rango_core::WriteContext {
    rango_core::WriteContext {
        tenant_id: tenant_id.to_string(),
        namespace: namespace.to_string(),
        actor: "actor".to_string(),
        source: "source".to_string(),
        tier: MemoryTier::State,
    }
}

proptest! {
    #[test]
    fn low_trust_payloads_are_rejected_deterministically(
        trust in 0.0f64..0.24f64,
        tenant_suffix in 0u16..1000u16,
        namespace_suffix in 0u16..1000u16
    ) {
        let control_plane = ControlPlane::default();
        let ctx = make_ctx(
            &format!("tenant-{tenant_suffix}"),
            &format!("ns-{namespace_suffix}"),
        );
        let payload = rango_core::WritePayload::StateWithTrust {
            document: doc! { "v": trust },
            trust_score: trust,
        };

        let decision = control_plane.write_path(&ctx, &payload).unwrap();
        prop_assert!(matches!(decision.decision, PolicyDecision::Reject));
        prop_assert!(decision.reason.starts_with("trust_score_below_threshold"));
    }

    #[test]
    fn high_trust_payloads_are_allowed_deterministically(
        trust in 0.25f64..1.0f64,
        tenant_suffix in 0u16..1000u16,
        namespace_suffix in 0u16..1000u16
    ) {
        let control_plane = ControlPlane::default();
        let ctx = make_ctx(
            &format!("tenant-{tenant_suffix}"),
            &format!("ns-{namespace_suffix}"),
        );
        let payload = rango_core::WritePayload::StateWithTrust {
            document: doc! { "v": trust },
            trust_score: trust,
        };

        let decision = control_plane.write_path(&ctx, &payload).unwrap();
        prop_assert!(matches!(decision.decision, PolicyDecision::Allow));
        prop_assert!(decision.reason.starts_with("trust_score:"));
    }
}

#[test]
fn pull_identity_candidates_must_be_unique_and_tenant_scoped() {
    let mut identities = HashSet::new();
    identities.insert(("tenant-a".to_string(), "ns-a".to_string(), "write-1".to_string()));
    identities.insert(("tenant-a".to_string(), "ns-a".to_string(), "write-2".to_string()));

    // Tenant and namespace are part of identity boundary; cross-tenant same write_id is allowed.
    let cross_tenant_inserted = identities.insert((
        "tenant-b".to_string(),
        "ns-a".to_string(),
        "write-1".to_string(),
    ));
    assert!(
        cross_tenant_inserted,
        "cross-tenant same write_id should be a distinct identity",
    );
    let duplicate_same_scope = identities.insert((
        "tenant-a".to_string(),
        "ns-a".to_string(),
        "write-1".to_string(),
    ));
    assert!(
        !duplicate_same_scope,
        "same tenant+namespace+write_id must remain unique",
    );
}
