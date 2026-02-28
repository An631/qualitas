/// Composite Quality Score computation
///
/// Formula:
///   For each pillar i:
///     penalty_i = saturation(raw_i) × weight_i × 100
///   Quality Score = max(0, 100 − Σ(penalty_i))
///
/// Saturation model (from PMC paper's saturation finding):
///   saturation(x) = 1 − e^(−K × x)
///   K = SATURATION_K = 0.15
///
/// This means at x=1.0 (exactly at the F-tier threshold), saturation ≈ 0.14.
/// The full 100-point scale is never reached by a single pillar — the composite
/// score degrades smoothly as pillars worsen.
use crate::constants::*;
use crate::types::*;

/// Apply the saturation function: 1 − e^(−k × x)
/// Returns a value in [0, 1).
pub fn saturate(x: f64) -> f64 {
    1.0 - (-SATURATION_K * x).exp()
}

/// Compute the composite Quality Score (0–100) from metric raw scores.
///
/// `weights` defaults to `WeightConfig::default()` if None.
/// `profile` is used to resolve weights when weights is None.
pub fn compute_score(
    metrics: &MetricBreakdown,
    weights: Option<&WeightConfig>,
    profile: Option<&str>,
) -> (f64, ScoreBreakdown) {
    let resolved_weights = weights
        .cloned()
        .unwrap_or_else(|| weights_for_profile(profile.unwrap_or("default")));

    // Normalize each pillar to a 0–∞ raw score, then saturate to [0, 1)
    let cfc_raw = metrics.cognitive_flow.score as f64 / NORM_CFC;
    let dci_raw = metrics.data_complexity.raw_score;
    let irc_raw = metrics.identifier_reference.total_irc / NORM_IRC;
    let dc_raw = metrics.dependency_coupling.raw_score;
    let sm_raw = metrics.structural.raw_score;

    // saturate returns ∈ [0, 1); multiply by 100 to get ∈ [0, 100) penalty
    let cfc_penalty = saturate(cfc_raw) * 100.0 * resolved_weights.cognitive_flow;
    let dci_penalty = saturate(dci_raw) * 100.0 * resolved_weights.data_complexity;
    let irc_penalty = saturate(irc_raw) * 100.0 * resolved_weights.identifier_reference;
    let dc_penalty = saturate(dc_raw) * 100.0 * resolved_weights.dependency_coupling;
    let sm_penalty = saturate(sm_raw) * 100.0 * resolved_weights.structural;

    let total_penalty = cfc_penalty + dci_penalty + irc_penalty + dc_penalty + sm_penalty;
    let score = (100.0 - total_penalty).max(0.0);

    let breakdown = ScoreBreakdown {
        cfc_penalty,
        dci_penalty,
        irc_penalty,
        dc_penalty,
        sm_penalty,
        total_penalty,
    };

    (score, breakdown)
}

/// Aggregate a list of function scores into a single file/class score.
/// Weighted by LOC so larger functions have proportionally more influence.
pub fn aggregate_scores(reports: &[(f64, u32)]) -> f64 {
    if reports.is_empty() {
        return 100.0;
    }
    let total_weight: u32 = reports.iter().map(|(_, loc)| loc.max(&1)).sum();
    if total_weight == 0 {
        return reports.iter().map(|(s, _)| s).sum::<f64>() / reports.len() as f64;
    }
    reports
        .iter()
        .map(|(score, loc)| score * (*loc.max(&1)) as f64)
        .sum::<f64>()
        / total_weight as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_metrics() -> MetricBreakdown {
        MetricBreakdown {
            cognitive_flow: CognitiveFlowResult {
                score: 0,
                nesting_penalty: 0,
                base_increments: 0,
                async_penalty: 0,
                max_nesting_depth: 0,
            },
            data_complexity: DataComplexityResult {
                halstead: HalsteadCounts {
                    distinct_operators: 0,
                    distinct_operands: 0,
                    total_operators: 0,
                    total_operands: 0,
                },
                difficulty: 0.0,
                volume: 0.0,
                effort: 0.0,
                raw_score: 0.0,
            },
            identifier_reference: IdentifierRefResult {
                total_irc: 0.0,
                hotspots: vec![],
            },
            dependency_coupling: DependencyCouplingResult {
                import_count: 0,
                distinct_sources: 0,
                external_ratio: 0.0,
                external_packages: vec![],
                internal_modules: vec![],
                distinct_api_calls: 0,
                closure_captures: 0,
                raw_score: 0.0,
            },
            structural: StructuralResult {
                loc: 0,
                total_lines: 0,
                parameter_count: 0,
                max_nesting_depth: 0,
                return_count: 0,
                method_count: None,
                raw_score: 0.0,
            },
        }
    }

    #[test]
    fn perfect_code_scores_100() {
        let metrics = zero_metrics();
        let (score, _) = compute_score(&metrics, None, None);
        assert!((score - 100.0).abs() < 0.01, "score was {score}");
    }

    #[test]
    fn high_cfc_reduces_score() {
        let mut metrics = zero_metrics();
        metrics.cognitive_flow.score = 30; // well past F-tier
        let (score, _) = compute_score(&metrics, None, None);
        assert!(score < 100.0);
        assert!(score > 0.0); // saturation means it never hits exactly 0
    }

    #[test]
    fn saturation_is_sublinear() {
        // Score at CFC=50 should not be twice as bad as at CFC=25
        let mut m1 = zero_metrics();
        m1.cognitive_flow.score = 25;
        let mut m2 = zero_metrics();
        m2.cognitive_flow.score = 50;

        let (s1, _) = compute_score(&m1, None, None);
        let (s2, _) = compute_score(&m2, None, None);

        let loss1 = 100.0 - s1;
        let loss2 = 100.0 - s2;
        // Loss at 50 should be less than 2× loss at 25 (saturation)
        assert!(loss2 < loss1 * 2.0, "loss1={loss1:.2}, loss2={loss2:.2}");
    }

    #[test]
    fn aggregate_weighted_by_loc() {
        // 10-line function with score 80, 90-line function with score 40
        // Weighted average ≈ (80×10 + 40×90) / 100 = 44
        let reports = vec![(80.0, 10), (40.0, 90)];
        let agg = aggregate_scores(&reports);
        assert!((agg - 44.0).abs() < 1.0, "agg={agg:.2}");
    }
}
