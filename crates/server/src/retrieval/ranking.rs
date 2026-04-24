use rango_types::{
    RANKING_FORMULA_V1, RankingExplainability, RetrievalCandidate, deterministic_score_v1,
};

pub fn rank_candidates_v1(mut candidates: Vec<RetrievalCandidate>) -> Vec<RetrievalCandidate> {
    for candidate in &mut candidates {
        let score = deterministic_score_v1(&candidate.signals);
        candidate.score = score;
        candidate.explainability = Some(RankingExplainability {
            formula_version: RANKING_FORMULA_V1.to_string(),
            relevance_weight: 0.35,
            trust_weight: 0.30,
            recency_weight: 0.20,
            provenance_weight: 0.15,
            relevance_component: 0.35 * candidate.signals.relevance,
            trust_component: 0.30 * candidate.signals.trust,
            recency_component: 0.20 * candidate.signals.recency,
            provenance_component: 0.15 * candidate.signals.provenance,
            total_score: score,
        });
    }

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.candidate_id.cmp(&b.candidate_id))
    });
    candidates
}
