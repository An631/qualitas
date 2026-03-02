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

/// Generate all applicable refactoring flags for a function report.
pub fn generate_flags(metrics: &MetricBreakdown) -> Vec<RefactoringFlag> {
    let mut flags = Vec::new();

    let cfc = metrics.cognitive_flow.score;
    if cfc >= CFC_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCognitiveFlow,
            severity: Severity::Error,
            message: format!("Cognitive flow complexity is {cfc} (threshold: {CFC_ERROR})"),
            suggestion: "Extract nested branches into separate named functions. Use early returns to flatten the nesting hierarchy.".to_string(),
            observed_value: f64::from(cfc),
            threshold: f64::from(CFC_ERROR),
        });
    } else if cfc >= CFC_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighCognitiveFlow,
            severity: Severity::Warning,
            message: format!("Cognitive flow complexity is {cfc} (threshold: {CFC_WARNING})"),
            suggestion: "Consider extracting complex conditional logic into helper functions."
                .to_string(),
            observed_value: f64::from(cfc),
            threshold: f64::from(CFC_WARNING),
        });
    }

    let dci_d = metrics.data_complexity.difficulty;
    if dci_d >= DCI_DIFFICULTY_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighDataComplexity,
            severity: Severity::Error,
            message: format!(
                "Halstead difficulty is {dci_d:.1} (threshold: {DCI_DIFFICULTY_ERROR})"
            ),
            suggestion: "Reduce the number of distinct operators and variables. Extract repeated computations into named constants or helper functions.".to_string(),
            observed_value: dci_d,
            threshold: DCI_DIFFICULTY_ERROR,
        });
    } else if dci_d >= DCI_DIFFICULTY_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighDataComplexity,
            severity: Severity::Warning,
            message: format!(
                "Halstead difficulty is {dci_d:.1} (threshold: {DCI_DIFFICULTY_WARNING})"
            ),
            suggestion: "Consider reducing variable density by splitting this function."
                .to_string(),
            observed_value: dci_d,
            threshold: DCI_DIFFICULTY_WARNING,
        });
    }

    let effort = metrics.data_complexity.effort;
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

    let irc = metrics.identifier_reference.total_irc;
    if irc >= IRC_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighIdentifierChurn,
            severity: Severity::Error,
            message: format!("Identifier reference complexity is {irc:.1} (threshold: {IRC_ERROR})"),
            suggestion: "Variables are referenced many times across a wide scope. Break this function into smaller functions to shorten variable lifetimes.".to_string(),
            observed_value: irc,
            threshold: IRC_ERROR,
        });
    } else if irc >= IRC_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::HighIdentifierChurn,
            severity: Severity::Warning,
            message: format!(
                "Identifier reference complexity is {irc:.1} (threshold: {IRC_WARNING})"
            ),
            suggestion: "Consider shortening variable scopes by extracting sub-functions."
                .to_string(),
            observed_value: irc,
            threshold: IRC_WARNING,
        });
    }

    let params = metrics.structural.parameter_count;
    if params >= PARAMS_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooManyParams,
            severity: Severity::Error,
            message: format!("Function has {params} parameters (threshold: {PARAMS_ERROR})"),
            suggestion:
                "Group related parameters into an options object: `{ option1, option2, ... }`."
                    .to_string(),
            observed_value: f64::from(params),
            threshold: f64::from(PARAMS_ERROR),
        });
    } else if params >= PARAMS_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::TooManyParams,
            severity: Severity::Warning,
            message: format!("Function has {params} parameters (threshold: {PARAMS_WARNING})"),
            suggestion: "Consider using an options object to reduce parameter count.".to_string(),
            observed_value: f64::from(params),
            threshold: f64::from(PARAMS_WARNING),
        });
    }

    let loc = metrics.structural.loc;
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

    let nesting = metrics.structural.max_nesting_depth;
    if nesting >= NESTING_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::DeepNesting,
            severity: Severity::Error,
            message: format!("Maximum nesting depth is {nesting} (threshold: {NESTING_ERROR})"),
            suggestion: "Use early returns (guard clauses) to flatten the nesting hierarchy."
                .to_string(),
            observed_value: f64::from(nesting),
            threshold: f64::from(NESTING_ERROR),
        });
    } else if nesting >= NESTING_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::DeepNesting,
            severity: Severity::Warning,
            message: format!("Maximum nesting depth is {nesting} (threshold: {NESTING_WARNING})"),
            suggestion: "Consider inverting conditions to reduce nesting.".to_string(),
            observed_value: f64::from(nesting),
            threshold: f64::from(NESTING_WARNING),
        });
    }

    let returns = metrics.structural.return_count;
    if returns >= RETURNS_ERROR {
        flags.push(RefactoringFlag {
            flag_type: FlagType::ExcessiveReturns,
            severity: Severity::Error,
            message: format!(
                "Function has {returns} return statements (threshold: {RETURNS_ERROR})"
            ),
            suggestion:
                "Consolidate return paths. Consider a single return with a result variable."
                    .to_string(),
            observed_value: f64::from(returns),
            threshold: f64::from(RETURNS_ERROR),
        });
    } else if returns >= RETURNS_WARNING {
        flags.push(RefactoringFlag {
            flag_type: FlagType::ExcessiveReturns,
            severity: Severity::Warning,
            message: format!(
                "Function has {returns} return statements (threshold: {RETURNS_WARNING})"
            ),
            suggestion: "Multiple return paths can make flow harder to follow.".to_string(),
            observed_value: f64::from(returns),
            threshold: f64::from(RETURNS_WARNING),
        });
    }

    let imports = metrics.dependency_coupling.import_count;
    let api_calls = metrics.dependency_coupling.distinct_api_calls;
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
