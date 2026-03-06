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

pub const PARAMS_WARNING: u32 = 5;
pub const PARAMS_ERROR: u32 = 7;

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

// ─── Resolved thresholds (per-language overrides merged with defaults) ────────

use crate::ir::language::ThresholdOverrides;

/// Thresholds with per-language overrides applied.
/// Created by merging `ThresholdOverrides` (from a language adapter)
/// with the global defaults defined above.
#[allow(dead_code)]
pub struct ResolvedThresholds {
    pub norm_cfc: f64,
    pub norm_dci_difficulty: f64,
    pub norm_dci_volume: f64,
    pub norm_irc: f64,
    pub norm_sm_loc: f64,
    pub norm_sm_params: f64,
    pub norm_sm_nesting: f64,
    pub norm_sm_returns: f64,
    pub cfc_warning: u32,
    pub cfc_error: u32,
    pub loc_warning: u32,
    pub loc_error: u32,
    pub params_warning: u32,
    pub params_error: u32,
    pub nesting_warning: u32,
    pub nesting_error: u32,
    pub returns_warning: u32,
    pub returns_error: u32,
}

#[allow(dead_code)]
impl ResolvedThresholds {
    /// Merge language-specific overrides with global defaults.
    pub fn from_overrides(overrides: Option<&ThresholdOverrides>) -> Self {
        match overrides {
            Some(o) => {
                let mut t = Self::defaults();
                t.apply_overrides(o);
                t
            }
            None => Self::defaults(),
        }
    }

    fn apply_overrides(&mut self, o: &ThresholdOverrides) {
        macro_rules! apply {
            ($field:ident) => {
                if let Some(v) = o.$field {
                    self.$field = v;
                }
            };
        }
        apply!(norm_cfc);
        apply!(norm_dci_difficulty);
        apply!(norm_dci_volume);
        apply!(norm_irc);
        apply!(norm_sm_loc);
        apply!(norm_sm_params);
        apply!(norm_sm_nesting);
        apply!(norm_sm_returns);
        apply!(cfc_warning);
        apply!(cfc_error);
        apply!(loc_warning);
        apply!(loc_error);
        apply!(params_warning);
        apply!(params_error);
        apply!(nesting_warning);
        apply!(nesting_error);
        apply!(returns_warning);
        apply!(returns_error);
    }

    /// All-defaults (equivalent to TypeScript thresholds).
    pub fn defaults() -> Self {
        Self {
            norm_cfc: NORM_CFC,
            norm_dci_difficulty: NORM_DCI_DIFFICULTY,
            norm_dci_volume: NORM_DCI_VOLUME,
            norm_irc: NORM_IRC,
            norm_sm_loc: NORM_SM_LOC,
            norm_sm_params: NORM_SM_PARAMS,
            norm_sm_nesting: NORM_SM_NESTING,
            norm_sm_returns: NORM_SM_RETURNS,
            cfc_warning: CFC_WARNING,
            cfc_error: CFC_ERROR,
            loc_warning: LOC_WARNING,
            loc_error: LOC_ERROR,
            params_warning: PARAMS_WARNING,
            params_error: PARAMS_ERROR,
            nesting_warning: NESTING_WARNING,
            nesting_error: NESTING_ERROR,
            returns_warning: RETURNS_WARNING,
            returns_error: RETURNS_ERROR,
        }
    }
}

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
            cognitive_flow: 0.15,
            data_complexity: 0.35,
            identifier_reference: 0.30,
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
