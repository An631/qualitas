/// Composite Quality Score computation
///
/// Formula:
///   For each pillar i:
///     `penalty_i` = `saturation(raw_i)` × `weight_i` × 100
///   Quality Score = max(0, 100 − `Σ(penalty_i)`)
///
/// Saturation model (from PMC paper's saturation finding):
///   saturation(x) = 1 − e^(−K × x)
///   K = `SATURATION_K` = 0.15
///
/// This means at x=1.0 (exactly at the F-tier threshold), saturation ≈ 0.14.
/// The full 100-point scale is never reached by a single pillar — the composite
/// score degrades smoothly as pillars worsen.
use crate::constants::{weights_for_profile, NORM_CFC, NORM_IRC, SATURATION_K};
use crate::types::{MetricBreakdown, ScoreBreakdown, WeightConfig};

/// Apply the saturation function: 1 − e^(−k × x)
/// Returns a value in [0, 1).
pub fn saturate(x: f64) -> f64 {
    1.0 - (-SATURATION_K * x).exp()
}

/// Compute the composite Quality Score (0–100) from metric raw scores.
///
/// `weights` defaults to `WeightConfig::default()` if None.
/// `profile` is used to resolve weights when weights is None.
fn pillar_penalty(raw: f64, weight: f64) -> f64 {
    saturate(raw) * 100.0 * weight
}

pub fn compute_score(
    metrics: &MetricBreakdown,
    weights: Option<&WeightConfig>,
    profile: Option<&str>,
) -> (f64, ScoreBreakdown) {
    let w = weights
        .cloned()
        .unwrap_or_else(|| weights_for_profile(profile.unwrap_or("default")));

    let breakdown = compute_breakdown(metrics, &w);
    let score = (100.0 - breakdown.total_penalty).max(0.0);
    (score, breakdown)
}

fn compute_breakdown(metrics: &MetricBreakdown, w: &WeightConfig) -> ScoreBreakdown {
    let cfc_penalty = pillar_penalty(
        f64::from(metrics.cognitive_flow.score) / NORM_CFC,
        w.cognitive_flow,
    );
    let dci_penalty = pillar_penalty(metrics.data_complexity.raw_score, w.data_complexity);
    let irc_penalty = pillar_penalty(
        metrics.identifier_reference.total_irc / NORM_IRC,
        w.identifier_reference,
    );
    let dc_penalty = pillar_penalty(metrics.dependency_coupling.raw_score, w.dependency_coupling);
    let sm_penalty = pillar_penalty(metrics.structural.raw_score, w.structural);

    ScoreBreakdown {
        cfc_penalty,
        dci_penalty,
        irc_penalty,
        dc_penalty,
        sm_penalty,
        total_penalty: cfc_penalty + dci_penalty + irc_penalty + dc_penalty + sm_penalty,
    }
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
        .map(|(score, loc)| score * f64::from(*loc.max(&1)))
        .sum::<f64>()
        / f64::from(total_weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_metrics() -> MetricBreakdown {
        MetricBreakdown::default()
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
