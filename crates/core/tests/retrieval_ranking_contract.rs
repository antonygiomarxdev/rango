use rango_types::{
    deterministic_score_v1, RankingExplainability, RankingSignals, RetrievalCandidate, RetrievalSource,
    RANKING_FORMULA_V1,
};

#[test]
fn ranking_formula_v1_is_deterministic_and_uses_all_required_signals() {
    let signals = RankingSignals {
        relevance: 0.80,
        trust: 0.70,
        recency: 0.50,
        provenance: 0.90,
    };
    let score = deterministic_score_v1(&signals);
    let expected = (0.35 * 0.80) + (0.30 * 0.70) + (0.20 * 0.50) + (0.15 * 0.90);
    assert!((score - expected).abs() < 1e-12);
}

#[test]
fn explainability_payload_has_component_breakdown_with_total() {
    let signals = RankingSignals {
        relevance: 0.4,
        trust: 0.9,
        recency: 0.3,
        provenance: 0.7,
    };
    let score = deterministic_score_v1(&signals);
    let explain = RankingExplainability {
        formula_version: RANKING_FORMULA_V1.to_string(),
        relevance_weight: 0.35,
        trust_weight: 0.30,
        recency_weight: 0.20,
        provenance_weight: 0.15,
        relevance_component: 0.35 * signals.relevance,
        trust_component: 0.30 * signals.trust,
        recency_component: 0.20 * signals.recency,
        provenance_component: 0.15 * signals.provenance,
        total_score: score,
    };
    let candidate = RetrievalCandidate {
        candidate_id: "cand-1".to_string(),
        tenant_id: "tenant-a".to_string(),
        namespace: "ns-a".to_string(),
        source: RetrievalSource::Vector,
        lineage: "write-1".to_string(),
        timestamp_ms: 1_710_000_000_000,
        payload: bson::doc! { "title": "doc-a" },
        signals,
        score,
        explainability: Some(explain.clone()),
    };

    assert_eq!(explain.formula_version, "v1");
    let reconstructed = explain.relevance_component
        + explain.trust_component
        + explain.recency_component
        + explain.provenance_component;
    assert!((reconstructed - explain.total_score).abs() < 1e-12);
    assert_eq!(candidate.explainability.unwrap().formula_version, "v1");
}
