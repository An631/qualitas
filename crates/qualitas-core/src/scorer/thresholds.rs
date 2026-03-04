use std::collections::HashMap;

use crate::constants::{
    grade_bounds_for_profile, API_CALLS_ERROR, API_CALLS_WARNING, CFC_ERROR, CFC_WARNING,
    DCI_DIFFICULTY_ERROR, DCI_DIFFICULTY_WARNING, HALSTEAD_EFFORT_ERROR, HALSTEAD_EFFORT_WARNING,
    IMPORT_ERROR, IMPORT_WARNING, IRC_ERROR, IRC_WARNING, LOC_ERROR, LOC_WARNING, NESTING_ERROR,
    NESTING_WARNING, PARAMS_ERROR, PARAMS_WARNING, RETURNS_ERROR, RETURNS_WARNING,
};
use crate::types::{FlagConfig, FlagType, Grade, MetricBreakdown, RefactoringFlag, Severity};

/// Assign a grade from a composite quality score.
pub fn grade_from_score(score: f64, profile: Option<&str>) -> Grade {
    let (a_min, b_min, c_min, d_min) = grade_bounds_for_profile(profile.unwrap_or("default"));

    if score >= a_min {
        Grade::A
    } else if score >= b_min {
        Grade::B
    } else if score >= c_min {
        Grade::C
    } else if score >= d_min {
        Grade::D
    } else {
        Grade::F
    }
}

// ─── Flag config resolution ──────────────────────────────────────────────────

struct ResolvedFlagThresholds {
    enabled: bool,
    warn: f64,
    error: f64,
}

fn resolve_flag(
    name: &str,
    default_enabled: bool,
    default_warn: f64,
    default_error: f64,
    overrides: Option<&HashMap<String, FlagConfig>>,
) -> ResolvedFlagThresholds {
    match overrides.and_then(|m| m.get(name)) {
        Some(FlagConfig::Enabled(true)) => ResolvedFlagThresholds {
            enabled: true,
            warn: default_warn,
            error: default_error,
        },
        Some(FlagConfig::Enabled(false)) => ResolvedFlagThresholds {
            enabled: false,
            warn: default_warn,
            error: default_error,
        },
        Some(FlagConfig::Custom { warn, error }) => ResolvedFlagThresholds {
            enabled: true,
            warn: *warn,
            error: *error,
        },
        None => ResolvedFlagThresholds {
            enabled: default_enabled,
            warn: default_warn,
            error: default_error,
        },
    }
}

// ─── Per-flag check functions ────────────────────────────────────────────────

/// Check cognitive flow complexity thresholds.
fn check_cfc_flags(score: u32, warn: f64, error: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    let val = f64::from(score);
    if val >= error {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCognitiveFlow,
            severity: Severity::Error,
            message: format!("Cognitive flow complexity is {score} (threshold: {error:.0})"),
            suggestion: "Extract nested branches into separate named functions. Use early returns to flatten the nesting hierarchy.".to_string(),
            observed_value: val,
            threshold: error,
        });
    } else if val >= warn {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCognitiveFlow,
            severity: Severity::Warning,
            message: format!("Cognitive flow complexity is {score} (threshold: {warn:.0})"),
            suggestion: "Consider extracting complex conditional logic into helper functions."
                .to_string(),
            observed_value: val,
            threshold: warn,
        });
    }
    flags
}

/// Check Halstead difficulty thresholds.
fn check_difficulty_flags(difficulty: f64, warn: f64, error: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if difficulty >= error {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighDataComplexity,
            severity: Severity::Error,
            message: format!("Halstead difficulty is {difficulty:.1} (threshold: {error:.0})"),
            suggestion: "Reduce the number of distinct operators and variables. Extract repeated computations into named constants or helper functions.".to_string(),
            observed_value: difficulty,
            threshold: error,
        });
    } else if difficulty >= warn {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighDataComplexity,
            severity: Severity::Warning,
            message: format!("Halstead difficulty is {difficulty:.1} (threshold: {warn:.0})"),
            suggestion: "Consider reducing variable density by splitting this function."
                .to_string(),
            observed_value: difficulty,
            threshold: warn,
        });
    }
    flags
}

/// Check Halstead effort thresholds.
fn check_halstead_effort_flags(effort: f64, warn: f64, error: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if effort >= error {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighHalsteadEffort,
            severity: Severity::Error,
            message: format!("Halstead effort is {effort:.0} (threshold: {error:.0})"),
            suggestion: "Simplify expressions. Extract complex calculations into well-named helper functions or constants.".to_string(),
            observed_value: effort,
            threshold: error,
        });
    } else if effort >= warn {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighHalsteadEffort,
            severity: Severity::Warning,
            message: format!("Halstead effort is {effort:.0} (threshold: {warn:.0})"),
            suggestion: "Consider simplifying this function's logic to reduce cognitive load."
                .to_string(),
            observed_value: effort,
            threshold: warn,
        });
    }
    flags
}

/// Check identifier reference complexity thresholds.
fn check_irc_flags(total_irc: f64, warn: f64, error: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if total_irc >= error {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighIdentifierChurn,
            severity: Severity::Error,
            message: format!("Identifier reference complexity is {total_irc:.1} (threshold: {error:.0})"),
            suggestion: "Variables are referenced many times across a wide scope. Break this function into smaller functions to shorten variable lifetimes.".to_string(),
            observed_value: total_irc,
            threshold: error,
        });
    } else if total_irc >= warn {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighIdentifierChurn,
            severity: Severity::Warning,
            message: format!(
                "Identifier reference complexity is {total_irc:.1} (threshold: {warn:.0})"
            ),
            suggestion: "Consider shortening variable scopes by extracting sub-functions."
                .to_string(),
            observed_value: total_irc,
            threshold: warn,
        });
    }
    flags
}

/// Check parameter count thresholds.
fn check_params_flags(count: u32, warn: f64, error: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    let val = f64::from(count);
    if val >= error {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooManyParams,
            severity: Severity::Error,
            message: format!("Function has {count} parameters (threshold: {error:.0})"),
            suggestion:
                "Group related parameters into an options object: `{ option1, option2, ... }`."
                    .to_string(),
            observed_value: val,
            threshold: error,
        });
    } else if val >= warn {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooManyParams,
            severity: Severity::Warning,
            message: format!("Function has {count} parameters (threshold: {warn:.0})"),
            suggestion: "Consider using an options object to reduce parameter count.".to_string(),
            observed_value: val,
            threshold: warn,
        });
    }
    flags
}

/// Check lines-of-code thresholds.
fn check_loc_flags(loc: u32, warn: f64, error: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    let val = f64::from(loc);
    if val >= error {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooLong,
            severity: Severity::Error,
            message: format!("Function is {loc} lines (threshold: {error:.0})"),
            suggestion: "Extract logical sub-operations into smaller named functions to keep each under 40 lines.".to_string(),
            observed_value: val,
            threshold: error,
        });
    } else if val >= warn {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooLong,
            severity: Severity::Warning,
            message: format!("Function is {loc} lines (threshold: {warn:.0})"),
            suggestion: "Consider breaking this function into smaller helpers.".to_string(),
            observed_value: val,
            threshold: warn,
        });
    }
    flags
}

/// Check maximum nesting depth thresholds.
fn check_nesting_flags(depth: u32, warn: f64, error: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    let val = f64::from(depth);
    if val >= error {
        flags.push(RefactoringFlag {
            flag_type: FlagType::DeepNesting,
            severity: Severity::Error,
            message: format!("Maximum nesting depth is {depth} (threshold: {error:.0})"),
            suggestion: "Use early returns (guard clauses) to flatten the nesting hierarchy."
                .to_string(),
            observed_value: val,
            threshold: error,
        });
    } else if val >= warn {
        flags.push(RefactoringFlag {
            flag_type: FlagType::DeepNesting,
            severity: Severity::Warning,
            message: format!("Maximum nesting depth is {depth} (threshold: {warn:.0})"),
            suggestion: "Consider inverting conditions to reduce nesting.".to_string(),
            observed_value: val,
            threshold: warn,
        });
    }
    flags
}

/// Check return statement count thresholds.
fn check_returns_flags(count: u32, warn: f64, error: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    let val = f64::from(count);
    if val >= error {
        flags.push(RefactoringFlag {
            flag_type: FlagType::ExcessiveReturns,
            severity: Severity::Error,
            message: format!("Function has {count} return statements (threshold: {error:.0})"),
            suggestion:
                "Consolidate return paths. Consider a single return with a result variable."
                    .to_string(),
            observed_value: val,
            threshold: error,
        });
    } else if val >= warn {
        flags.push(RefactoringFlag {
            flag_type: FlagType::ExcessiveReturns,
            severity: Severity::Warning,
            message: format!("Function has {count} return statements (threshold: {warn:.0})"),
            suggestion: "Multiple return paths can make flow harder to follow.".to_string(),
            observed_value: val,
            threshold: warn,
        });
    }
    flags
}

/// Check import count and API call coupling thresholds.
fn check_coupling_flags(
    imports: u32,
    api_calls: u32,
    warn: f64,
    error: f64,
) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    let import_val = f64::from(imports);
    let api_val = f64::from(api_calls);
    if import_val >= error || api_val >= f64::from(API_CALLS_ERROR) {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCoupling,
            severity: Severity::Error,
            message: format!("High coupling: {imports} imports, {api_calls} distinct API calls"),
            suggestion: "Consider splitting this module. Single Responsibility Principle: each module should have one reason to change.".to_string(),
            observed_value: import_val,
            threshold: error,
        });
    } else if import_val >= warn || api_val >= f64::from(API_CALLS_WARNING) {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCoupling,
            severity: Severity::Warning,
            message: format!(
                "Moderate coupling: {imports} imports, {api_calls} distinct API calls"
            ),
            suggestion:
                "Review whether all imports are necessary; consider grouping related functionality."
                    .to_string(),
            observed_value: import_val,
            threshold: warn,
        });
    }
    flags
}

/// Generate all applicable refactoring flags for a function report.
///
/// When `flag_overrides` is `Some`, each flag is resolved against the config:
///   - `FlagConfig::Enabled(false)` → flag is skipped
///   - `FlagConfig::Enabled(true)` → flag uses built-in thresholds
///   - `FlagConfig::Custom { warn, error }` → flag uses custom thresholds
///   - Not present → uses built-in defaults (all enabled except `excessiveReturns`)
#[allow(clippy::implicit_hasher)]
pub fn generate_flags(
    metrics: &MetricBreakdown,
    flag_overrides: Option<&HashMap<String, FlagConfig>>,
) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    let o = flag_overrides;

    macro_rules! check {
        ($n:expr, $en:expr, $w:expr, $e:expr, $chk:expr) => {{
            let r = resolve_flag($n, $en, $w, $e, o);
            if r.enabled {
                flags.extend($chk(r.warn, r.error));
            }
        }};
    }

    check!(
        "highCognitiveFlow",
        true,
        f64::from(CFC_WARNING),
        f64::from(CFC_ERROR),
        |w, e| check_cfc_flags(metrics.cognitive_flow.score, w, e)
    );
    check!(
        "highDataComplexity",
        true,
        DCI_DIFFICULTY_WARNING,
        DCI_DIFFICULTY_ERROR,
        |w, e| check_difficulty_flags(metrics.data_complexity.difficulty, w, e)
    );
    check!(
        "highHalsteadEffort",
        true,
        HALSTEAD_EFFORT_WARNING,
        HALSTEAD_EFFORT_ERROR,
        |w, e| check_halstead_effort_flags(metrics.data_complexity.effort, w, e)
    );
    check!(
        "highIdentifierChurn",
        true,
        IRC_WARNING,
        IRC_ERROR,
        |w, e| check_irc_flags(metrics.identifier_reference.total_irc, w, e)
    );
    check!(
        "tooManyParams",
        true,
        f64::from(PARAMS_WARNING),
        f64::from(PARAMS_ERROR),
        |w, e| check_params_flags(metrics.structural.parameter_count, w, e)
    );
    check!(
        "tooLong",
        true,
        f64::from(LOC_WARNING),
        f64::from(LOC_ERROR),
        |w, e| check_loc_flags(metrics.structural.loc, w, e)
    );
    check!(
        "deepNesting",
        true,
        f64::from(NESTING_WARNING),
        f64::from(NESTING_ERROR),
        |w, e| check_nesting_flags(metrics.structural.max_nesting_depth, w, e)
    );
    check!(
        "excessiveReturns",
        false,
        f64::from(RETURNS_WARNING),
        f64::from(RETURNS_ERROR),
        |w, e| check_returns_flags(metrics.structural.return_count, w, e)
    );
    check!(
        "highCoupling",
        true,
        f64::from(IMPORT_WARNING),
        f64::from(IMPORT_ERROR),
        |w, e| check_coupling_flags(
            metrics.dependency_coupling.import_count,
            metrics.dependency_coupling.distinct_api_calls,
            w,
            e
        )
    );

    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        CognitiveFlowResult, DataComplexityResult, DependencyCouplingResult, FlagConfig, Grade,
        HalsteadCounts, IdentifierRefResult, MetricBreakdown, StructuralResult,
    };

    /// Build a MetricBreakdown with all metrics at zero (clean code).
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

    // ── Grade boundary tests ──────────────────────────────────────────────

    #[test]
    fn grade_a_for_high_score() {
        let grade = grade_from_score(85.0, None);
        assert_eq!(grade, Grade::A, "Score 85 should be grade A");
    }

    #[test]
    fn grade_b_for_medium_score() {
        let grade = grade_from_score(70.0, None);
        assert_eq!(grade, Grade::B, "Score 70 should be grade B");
    }

    #[test]
    fn grade_f_for_very_low_score() {
        let grade = grade_from_score(20.0, None);
        assert_eq!(grade, Grade::F, "Score 20 should be grade F");
    }

    #[test]
    fn strict_profile_has_tighter_bounds() {
        // With strict profile: A requires >= 90, so 85 should be B
        let grade = grade_from_score(85.0, Some("strict"));
        assert_eq!(
            grade,
            Grade::B,
            "Score 85 with strict profile should be grade B (not A)",
        );
    }

    // ── Flag generation tests ─────────────────────────────────────────────

    #[test]
    fn no_flags_for_clean_metrics() {
        let metrics = zero_metrics();
        let flags = generate_flags(&metrics, None);
        assert!(
            flags.is_empty(),
            "Expected no flags for zero metrics, got {} flags",
            flags.len(),
        );
    }

    #[test]
    fn cfc_warning_at_threshold() {
        let mut metrics = zero_metrics();
        metrics.cognitive_flow.score = 13; // CFC_WARNING = 13
        let flags = generate_flags(&metrics, None);
        assert!(!flags.is_empty(), "Expected at least one flag for CFC=13",);
        let cfc_flag = flags
            .iter()
            .find(|f| f.flag_type == FlagType::HighCognitiveFlow)
            .expect("Expected a HighCognitiveFlow flag");
        assert_eq!(
            cfc_flag.severity,
            Severity::Warning,
            "CFC=13 should be a warning, not {:?}",
            cfc_flag.severity,
        );
    }

    #[test]
    fn cfc_error_above_threshold() {
        let mut metrics = zero_metrics();
        metrics.cognitive_flow.score = 20; // >= CFC_ERROR (19)
        let flags = generate_flags(&metrics, None);
        let cfc_flag = flags
            .iter()
            .find(|f| f.flag_type == FlagType::HighCognitiveFlow)
            .expect("Expected a HighCognitiveFlow flag");
        assert_eq!(
            cfc_flag.severity,
            Severity::Error,
            "CFC=20 should be an error, not {:?}",
            cfc_flag.severity,
        );
    }

    #[test]
    fn loc_warning_at_threshold() {
        let mut metrics = zero_metrics();
        metrics.structural.loc = 41; // LOC_WARNING = 41
        let flags = generate_flags(&metrics, None);
        let loc_flag = flags
            .iter()
            .find(|f| f.flag_type == FlagType::TooLong)
            .expect("Expected a TooLong flag");
        assert_eq!(
            loc_flag.severity,
            Severity::Warning,
            "LOC=41 should be a warning, not {:?}",
            loc_flag.severity,
        );
    }

    #[test]
    fn params_error_at_threshold() {
        let mut metrics = zero_metrics();
        metrics.structural.parameter_count = 5; // PARAMS_ERROR = 5
        let flags = generate_flags(&metrics, None);
        let params_flag = flags
            .iter()
            .find(|f| f.flag_type == FlagType::TooManyParams)
            .expect("Expected a TooManyParams flag");
        assert_eq!(
            params_flag.severity,
            Severity::Error,
            "params=5 should be an error, not {:?}",
            params_flag.severity,
        );
    }

    #[test]
    fn multiple_flags_for_complex_metrics() {
        let mut metrics = zero_metrics();
        metrics.cognitive_flow.score = 20; // triggers CFC error
        metrics.structural.loc = 65; // triggers LOC error
        let flags = generate_flags(&metrics, None);
        assert!(
            flags.len() >= 2,
            "Expected at least 2 flags for high CFC + high LOC, got {}",
            flags.len(),
        );
        let has_cfc = flags
            .iter()
            .any(|f| f.flag_type == FlagType::HighCognitiveFlow);
        let has_loc = flags.iter().any(|f| f.flag_type == FlagType::TooLong);
        assert!(has_cfc, "Expected a HighCognitiveFlow flag");
        assert!(has_loc, "Expected a TooLong flag");
    }

    // ── Flag config tests ─────────────────────────────────────────────────

    #[test]
    fn excessive_returns_disabled_by_default() {
        let mut metrics = zero_metrics();
        metrics.structural.return_count = 10; // well above threshold
        let flags = generate_flags(&metrics, None);
        assert!(
            !flags
                .iter()
                .any(|f| f.flag_type == FlagType::ExcessiveReturns),
            "excessiveReturns should be disabled by default",
        );
    }

    #[test]
    fn excessive_returns_enabled_via_config() {
        let mut metrics = zero_metrics();
        metrics.structural.return_count = 5; // above error threshold (4)
        let mut overrides = HashMap::new();
        overrides.insert("excessiveReturns".to_string(), FlagConfig::Enabled(true));
        let flags = generate_flags(&metrics, Some(&overrides));
        assert!(
            flags
                .iter()
                .any(|f| f.flag_type == FlagType::ExcessiveReturns),
            "excessiveReturns should fire when explicitly enabled",
        );
    }

    #[test]
    fn flag_disabled_via_config() {
        let mut metrics = zero_metrics();
        metrics.cognitive_flow.score = 20; // would normally trigger CFC error
        let mut overrides = HashMap::new();
        overrides.insert("highCognitiveFlow".to_string(), FlagConfig::Enabled(false));
        let flags = generate_flags(&metrics, Some(&overrides));
        assert!(
            !flags
                .iter()
                .any(|f| f.flag_type == FlagType::HighCognitiveFlow),
            "highCognitiveFlow should be suppressed when disabled",
        );
    }

    #[test]
    fn custom_thresholds_applied() {
        let mut metrics = zero_metrics();
        metrics.structural.loc = 50; // above default warning (41) but below custom warn (60)
        let mut overrides = HashMap::new();
        overrides.insert(
            "tooLong".to_string(),
            FlagConfig::Custom {
                warn: 60.0,
                error: 100.0,
            },
        );
        let flags = generate_flags(&metrics, Some(&overrides));
        assert!(
            !flags.iter().any(|f| f.flag_type == FlagType::TooLong),
            "LOC=50 should not trigger tooLong with custom warn=60",
        );

        // Now push above custom warn
        metrics.structural.loc = 65;
        let flags = generate_flags(&metrics, Some(&overrides));
        let loc_flag = flags
            .iter()
            .find(|f| f.flag_type == FlagType::TooLong)
            .expect("LOC=65 should trigger tooLong with custom warn=60");
        assert_eq!(loc_flag.severity, Severity::Warning);
        assert!(
            (loc_flag.threshold - 60.0).abs() < 0.01,
            "threshold should be custom warn=60"
        );
    }
}
