use crate::types::WeightConfig;

// ─── Saturation model ────────────────────────────────────────────────────────
// saturation(x) = MAX_PENALTY × (1 − e^(−K × x))
//
// K=1.0 was calibrated so that at x=1 (hitting the F-tier threshold exactly)
// the function returns ≈63% of MAX_PENALTY, and at x=2 (twice F-tier) ≈86%.
// This matches the paper's finding that saturation occurs quickly — once code
// passes the complexity threshold, developers experience near-maximum difficulty.
// Doubling already-F-tier code doesn't double perceived difficulty (sublinear),
// but the bulk of the penalty is felt immediately past the threshold.
//
// MAX_PENALTY = 100 per pillar, weights scale down contributions.

pub const SATURATION_K: f64 = 1.0;

// ─── Normalization divisors ────────────────────────────────────────────────
// These are the "F-tier" raw values that normalize to 1.0.
// Values above produce x > 1.0 which saturates quickly.

pub const NORM_CFC: f64 = 25.0;
pub const NORM_DCI_DIFFICULTY: f64 = 60.0;
pub const NORM_DCI_VOLUME: f64 = 3000.0;
pub const NORM_IRC: f64 = 100.0;
pub const NORM_DC_IMPORTS: f64 = 20.0;
pub const NORM_DC_API_CALLS: f64 = 15.0;
pub const NORM_SM_LOC: f64 = 100.0;
pub const NORM_SM_PARAMS: f64 = 6.0;
pub const NORM_SM_NESTING: f64 = 6.0;
pub const NORM_SM_RETURNS: f64 = 5.0;

// ─── DCI sub-weights ─────────────────────────────────────────────────────────
pub const DCI_DIFFICULTY_WEIGHT: f64 = 0.60;
pub const DCI_VOLUME_WEIGHT: f64 = 0.40;

// ─── DC sub-weights ──────────────────────────────────────────────────────────
pub const DC_IMPORT_WEIGHT: f64 = 0.40;
pub const DC_EXTERNAL_RATIO_WEIGHT: f64 = 0.30;
pub const DC_API_CALLS_WEIGHT: f64 = 0.30;

// ─── SM sub-weights ──────────────────────────────────────────────────────────
pub const SM_LOC_WEIGHT: f64 = 0.40;
pub const SM_PARAMS_WEIGHT: f64 = 0.30;
pub const SM_NESTING_WEIGHT: f64 = 0.20;
pub const SM_RETURNS_WEIGHT: f64 = 0.10;

// ─── Default grade boundaries (composite score) ───────────────────────────────
// Grade A: score >= A_MIN, B: >= B_MIN, etc.
pub const GRADE_A_MIN: f64 = 80.0;
pub const GRADE_B_MIN: f64 = 65.0;
pub const GRADE_C_MIN: f64 = 50.0;
pub const GRADE_D_MIN: f64 = 35.0;
// Below D_MIN = F

// ─── Default refactoring threshold ───────────────────────────────────────────
pub const DEFAULT_REFACTORING_THRESHOLD: f64 = 65.0; // grade B boundary

// ─── Per-metric flag thresholds (function level) ─────────────────────────────
// Values at or above "C" boundary trigger a warning flag
// Values at or above "D" boundary trigger an error flag

pub const CFC_WARNING: u32 = 13;
pub const CFC_ERROR: u32 = 19;

pub const DCI_DIFFICULTY_WARNING: f64 = 26.0;
pub const DCI_DIFFICULTY_ERROR: f64 = 41.0;
pub const HALSTEAD_EFFORT_WARNING: f64 = 1500.0;
pub const HALSTEAD_EFFORT_ERROR: f64 = 5000.0;

pub const IRC_WARNING: f64 = 41.0;
pub const IRC_ERROR: f64 = 71.0;

pub const PARAMS_WARNING: u32 = 4;
pub const PARAMS_ERROR: u32 = 5;

pub const LOC_WARNING: u32 = 41;
pub const LOC_ERROR: u32 = 61;

pub const NESTING_WARNING: u32 = 4;
pub const NESTING_ERROR: u32 = 5;

pub const RETURNS_WARNING: u32 = 3;
pub const RETURNS_ERROR: u32 = 4;

pub const IMPORT_WARNING: u32 = 10;
pub const IMPORT_ERROR: u32 = 15;
pub const API_CALLS_WARNING: u32 = 8;
pub const API_CALLS_ERROR: u32 = 12;

// ─── Named weight profiles ────────────────────────────────────────────────────

pub fn weights_for_profile(profile: &str) -> WeightConfig {
    match profile {
        "cc-focused" => WeightConfig {
            cognitive_flow: 0.50,
            data_complexity: 0.20,
            identifier_reference: 0.15,
            dependency_coupling: 0.10,
            structural: 0.05,
        },
        "data-focused" => WeightConfig {
            cognitive_flow: 0.20,
            data_complexity: 0.35,
            identifier_reference: 0.25,
            dependency_coupling: 0.12,
            structural: 0.08,
        },
        "strict" => WeightConfig::default(), // same weights, tighter grade bands
        _ => WeightConfig::default(),        // "default"
    }
}

pub fn grade_bounds_for_profile(profile: &str) -> (f64, f64, f64, f64) {
    // returns (A_min, B_min, C_min, D_min)
    if profile == "strict" {
        (90.0, 75.0, 60.0, 40.0)
    } else {
        (GRADE_A_MIN, GRADE_B_MIN, GRADE_C_MIN, GRADE_D_MIN)
    }
}
