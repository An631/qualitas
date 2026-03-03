use crate::constants::{
    grade_bounds_for_profile, API_CALLS_ERROR, API_CALLS_WARNING, CFC_ERROR, CFC_WARNING,
    DCI_DIFFICULTY_ERROR, DCI_DIFFICULTY_WARNING, HALSTEAD_EFFORT_ERROR, HALSTEAD_EFFORT_WARNING,
    IMPORT_ERROR, IMPORT_WARNING, IRC_ERROR, IRC_WARNING, LOC_ERROR, LOC_WARNING, NESTING_ERROR,
    NESTING_WARNING, PARAMS_ERROR, PARAMS_WARNING, RETURNS_ERROR, RETURNS_WARNING,
};
use crate::types::{FlagType, Grade, MetricBreakdown, RefactoringFlag, Severity};

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

/// Check cognitive flow complexity thresholds.
fn check_cfc_flags(score: u32) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if score >= CFC_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCognitiveFlow,
            severity: Severity::Error,
            message: format!("Cognitive flow complexity is {score} (threshold: {CFC_ERROR})"),
            suggestion: "Extract nested branches into separate named functions. Use early returns to flatten the nesting hierarchy.".to_string(),
            observed_value: f64::from(score),
            threshold: f64::from(CFC_ERROR),
        });
    } else if score >= CFC_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCognitiveFlow,
            severity: Severity::Warning,
            message: format!("Cognitive flow complexity is {score} (threshold: {CFC_WARNING})"),
            suggestion: "Consider extracting complex conditional logic into helper functions."
                .to_string(),
            observed_value: f64::from(score),
            threshold: f64::from(CFC_WARNING),
        });
    }
    flags
}

/// Check Halstead difficulty and effort thresholds.
fn check_dci_flags(difficulty: f64, effort: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if difficulty >= DCI_DIFFICULTY_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighDataComplexity,
            severity: Severity::Error,
            message: format!(
                "Halstead difficulty is {difficulty:.1} (threshold: {DCI_DIFFICULTY_ERROR})"
            ),
            suggestion: "Reduce the number of distinct operators and variables. Extract repeated computations into named constants or helper functions.".to_string(),
            observed_value: difficulty,
            threshold: DCI_DIFFICULTY_ERROR,
        });
    } else if difficulty >= DCI_DIFFICULTY_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighDataComplexity,
            severity: Severity::Warning,
            message: format!(
                "Halstead difficulty is {difficulty:.1} (threshold: {DCI_DIFFICULTY_WARNING})"
            ),
            suggestion: "Consider reducing variable density by splitting this function."
                .to_string(),
            observed_value: difficulty,
            threshold: DCI_DIFFICULTY_WARNING,
        });
    }

    if effort >= HALSTEAD_EFFORT_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighHalsteadEffort,
            severity: Severity::Error,
            message: format!("Halstead effort is {effort:.0} (threshold: {HALSTEAD_EFFORT_ERROR})"),
            suggestion: "Simplify expressions. Extract complex calculations into well-named helper functions or constants.".to_string(),
            observed_value: effort,
            threshold: HALSTEAD_EFFORT_ERROR,
        });
    } else if effort >= HALSTEAD_EFFORT_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighHalsteadEffort,
            severity: Severity::Warning,
            message: format!(
                "Halstead effort is {effort:.0} (threshold: {HALSTEAD_EFFORT_WARNING})"
            ),
            suggestion: "Consider simplifying this function's logic to reduce cognitive load."
                .to_string(),
            observed_value: effort,
            threshold: HALSTEAD_EFFORT_WARNING,
        });
    }
    flags
}

/// Check identifier reference complexity thresholds.
fn check_irc_flags(total_irc: f64) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if total_irc >= IRC_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighIdentifierChurn,
            severity: Severity::Error,
            message: format!("Identifier reference complexity is {total_irc:.1} (threshold: {IRC_ERROR})"),
            suggestion: "Variables are referenced many times across a wide scope. Break this function into smaller functions to shorten variable lifetimes.".to_string(),
            observed_value: total_irc,
            threshold: IRC_ERROR,
        });
    } else if total_irc >= IRC_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighIdentifierChurn,
            severity: Severity::Warning,
            message: format!(
                "Identifier reference complexity is {total_irc:.1} (threshold: {IRC_WARNING})"
            ),
            suggestion: "Consider shortening variable scopes by extracting sub-functions."
                .to_string(),
            observed_value: total_irc,
            threshold: IRC_WARNING,
        });
    }
    flags
}

/// Check parameter count thresholds.
fn check_params_flags(count: u32) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if count >= PARAMS_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooManyParams,
            severity: Severity::Error,
            message: format!("Function has {count} parameters (threshold: {PARAMS_ERROR})"),
            suggestion:
                "Group related parameters into an options object: `{ option1, option2, ... }`."
                    .to_string(),
            observed_value: f64::from(count),
            threshold: f64::from(PARAMS_ERROR),
        });
    } else if count >= PARAMS_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooManyParams,
            severity: Severity::Warning,
            message: format!("Function has {count} parameters (threshold: {PARAMS_WARNING})"),
            suggestion: "Consider using an options object to reduce parameter count.".to_string(),
            observed_value: f64::from(count),
            threshold: f64::from(PARAMS_WARNING),
        });
    }
    flags
}

/// Check lines-of-code thresholds.
fn check_loc_flags(loc: u32) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if loc >= LOC_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooLong,
            severity: Severity::Error,
            message: format!("Function is {loc} lines (threshold: {LOC_ERROR})"),
            suggestion: "Extract logical sub-operations into smaller named functions to keep each under 40 lines.".to_string(),
            observed_value: f64::from(loc),
            threshold: f64::from(LOC_ERROR),
        });
    } else if loc >= LOC_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooLong,
            severity: Severity::Warning,
            message: format!("Function is {loc} lines (threshold: {LOC_WARNING})"),
            suggestion: "Consider breaking this function into smaller helpers.".to_string(),
            observed_value: f64::from(loc),
            threshold: f64::from(LOC_WARNING),
        });
    }
    flags
}

/// Check maximum nesting depth thresholds.
fn check_nesting_flags(depth: u32) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if depth >= NESTING_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::DeepNesting,
            severity: Severity::Error,
            message: format!("Maximum nesting depth is {depth} (threshold: {NESTING_ERROR})"),
            suggestion: "Use early returns (guard clauses) to flatten the nesting hierarchy."
                .to_string(),
            observed_value: f64::from(depth),
            threshold: f64::from(NESTING_ERROR),
        });
    } else if depth >= NESTING_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::DeepNesting,
            severity: Severity::Warning,
            message: format!("Maximum nesting depth is {depth} (threshold: {NESTING_WARNING})"),
            suggestion: "Consider inverting conditions to reduce nesting.".to_string(),
            observed_value: f64::from(depth),
            threshold: f64::from(NESTING_WARNING),
        });
    }
    flags
}

/// Check return statement count thresholds.
fn check_returns_flags(count: u32) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if count >= RETURNS_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::ExcessiveReturns,
            severity: Severity::Error,
            message: format!("Function has {count} return statements (threshold: {RETURNS_ERROR})"),
            suggestion:
                "Consolidate return paths. Consider a single return with a result variable."
                    .to_string(),
            observed_value: f64::from(count),
            threshold: f64::from(RETURNS_ERROR),
        });
    } else if count >= RETURNS_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::ExcessiveReturns,
            severity: Severity::Warning,
            message: format!(
                "Function has {count} return statements (threshold: {RETURNS_WARNING})"
            ),
            suggestion: "Multiple return paths can make flow harder to follow.".to_string(),
            observed_value: f64::from(count),
            threshold: f64::from(RETURNS_WARNING),
        });
    }
    flags
}

/// Check import count and API call coupling thresholds.
fn check_coupling_flags(imports: u32, api_calls: u32) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();
    if imports >= IMPORT_ERROR || api_calls >= API_CALLS_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCoupling,
            severity: Severity::Error,
            message: format!("High coupling: {imports} imports, {api_calls} distinct API calls"),
            suggestion: "Consider splitting this module. Single Responsibility Principle: each module should have one reason to change.".to_string(),
            observed_value: f64::from(imports),
            threshold: f64::from(IMPORT_ERROR),
        });
    } else if imports >= IMPORT_WARNING || api_calls >= API_CALLS_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCoupling,
            severity: Severity::Warning,
            message: format!(
                "Moderate coupling: {imports} imports, {api_calls} distinct API calls"
            ),
            suggestion:
                "Review whether all imports are necessary; consider grouping related functionality."
                    .to_string(),
            observed_value: f64::from(imports),
            threshold: f64::from(IMPORT_WARNING),
        });
    }
    flags
}

/// Generate all applicable refactoring flags for a function report.
pub fn generate_flags(metrics: &MetricBreakdown) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();

    flags.extend(check_cfc_flags(metrics.cognitive_flow.score));
    flags.extend(check_dci_flags(
        metrics.data_complexity.difficulty,
        metrics.data_complexity.effort,
    ));
    flags.extend(check_irc_flags(metrics.identifier_reference.total_irc));
    flags.extend(check_params_flags(metrics.structural.parameter_count));
    flags.extend(check_loc_flags(metrics.structural.loc));
    flags.extend(check_nesting_flags(metrics.structural.max_nesting_depth));
    flags.extend(check_returns_flags(metrics.structural.return_count));
    flags.extend(check_coupling_flags(
        metrics.dependency_coupling.import_count,
        metrics.dependency_coupling.distinct_api_calls,
    ));

    flags
}
