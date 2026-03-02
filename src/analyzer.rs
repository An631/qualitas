/// Main analysis orchestrator — language-agnostic.
///
/// Uses the language adapter registry to detect the language from the file
/// extension, extract functions/classes with IR events, and run the 5 metric
/// collectors on the event streams.
use crate::ir::language::{ClassExtraction, FileExtraction, FunctionExtraction};
use crate::languages::adapter_for_file;
use crate::metrics::{
    cognitive_flow::compute_cfc,
    data_complexity::compute_dci,
    dependencies::{analyze_file_dependencies_ir, compute_dc_from_events},
    identifier_refs::compute_irc,
    structural::{compute_sm_from_events, compute_sm_raw},
};
use crate::scorer::{
    composite::{aggregate_scores, compute_score},
    thresholds::{generate_flags, grade_from_score},
};
use crate::types::*;

pub fn analyze_source_str(
    source: &str,
    file_path: &str,
    options: &AnalysisOptions,
) -> Result<FileQualityReport, String> {
    // 1. Detect language and extract functions/classes/imports
    let adapter = adapter_for_file(file_path)?;
    let extraction = adapter.extract(source, file_path)?;

    let profile = options.profile.as_deref();
    let weights_ref = options.weights.as_ref();
    let refactoring_threshold = options
        .refactoring_threshold
        .unwrap_or(crate::constants::DEFAULT_REFACTORING_THRESHOLD);

    // 2. File-level dependency analysis from imports
    let file_deps = analyze_file_dependencies_ir(&extraction.imports);

    // 3. Destructure extraction so we can move parts independently
    let FileExtraction {
        functions,
        classes,
        imports,
    } = extraction;

    let fn_count = functions.len() as u32
        + classes.iter().map(|c| c.methods.len() as u32).sum::<u32>();
    let class_count = classes.len() as u32;

    // 4. Build per-function reports from event streams
    let function_reports: Vec<FunctionQualityReport> = functions
        .into_iter()
        .map(|fe| {
            build_fn_report_from_events(
                fe,
                source,
                &imports,
                profile,
                weights_ref,
                refactoring_threshold,
            )
        })
        .collect();

    // 5. Build class reports
    let class_reports: Vec<ClassQualityReport> = classes
        .into_iter()
        .map(|ce| {
            build_class_report_from_events(
                ce,
                source,
                &imports,
                profile,
                weights_ref,
                refactoring_threshold,
            )
        })
        .collect();

    // 6. File-level score (LOC-weighted average)
    let mut all_scores: Vec<(f64, u32)> = function_reports
        .iter()
        .map(|r| (r.score, r.metrics.structural.loc.max(1)))
        .collect();
    for cr in &class_reports {
        for mr in &cr.methods {
            all_scores.push((mr.score, mr.metrics.structural.loc.max(1)));
        }
    }

    let file_score = if all_scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&all_scores)
    };

    let grade = grade_from_score(file_score, profile);
    let needs_refactoring = file_score < refactoring_threshold;
    let flagged_fn_count = function_reports
        .iter()
        .filter(|r| r.needs_refactoring)
        .count() as u32;

    Ok(FileQualityReport {
        file_path: file_path.to_string(),
        score: file_score,
        grade,
        needs_refactoring,
        flags: vec![],
        functions: function_reports,
        classes: class_reports,
        file_dependencies: file_deps,
        total_lines: source.lines().count() as u32,
        function_count: fn_count,
        class_count,
        flagged_function_count: flagged_fn_count,
    })
}

// ─── Report assembly from event streams ─────────────────────────────────────

fn build_fn_report_from_events(
    fe: FunctionExtraction,
    source: &str,
    imports: &[crate::ir::language::ImportRecord],
    profile: Option<&str>,
    weights: Option<&WeightConfig>,
    refactoring_threshold: f64,
) -> FunctionQualityReport {
    let cfc = compute_cfc(&fe.events);
    let dci = compute_dci(&fe.events);
    let irc = compute_irc(&fe.events, source);
    let dc = compute_dc_from_events(&fe.events, imports);
    let sm = compute_sm_from_events(&fe.events, source, fe.byte_start, fe.byte_end, fe.param_count);

    let metrics = MetricBreakdown {
        cognitive_flow: cfc,
        data_complexity: dci,
        identifier_reference: irc,
        dependency_coupling: dc,
        structural: sm,
    };

    let (score, breakdown) = compute_score(&metrics, weights, profile);
    let grade = grade_from_score(score, profile);
    let needs_refactoring = score < refactoring_threshold;
    let flags = generate_flags(&metrics);

    FunctionQualityReport {
        name: fe.name,
        inferred_name: fe.inferred_name,
        score,
        grade,
        needs_refactoring,
        flags,
        metrics,
        score_breakdown: breakdown,
        location: SourceLocation {
            file: String::new(),
            start_line: fe.start_line,
            end_line: fe.end_line,
            start_col: 0,
            end_col: 0,
        },
        is_async: fe.is_async,
        is_generator: fe.is_generator,
    }
}

fn build_class_report_from_events(
    ce: ClassExtraction,
    source: &str,
    imports: &[crate::ir::language::ImportRecord],
    profile: Option<&str>,
    weights: Option<&WeightConfig>,
    refactoring_threshold: f64,
) -> ClassQualityReport {
    let method_reports: Vec<FunctionQualityReport> = ce
        .methods
        .into_iter()
        .map(|m| {
            build_fn_report_from_events(m, source, imports, profile, weights, refactoring_threshold)
        })
        .collect();

    let method_scores: Vec<(f64, u32)> = method_reports
        .iter()
        .map(|r| (r.score, r.metrics.structural.loc.max(1)))
        .collect();

    let class_score = if method_scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&method_scores)
    };

    let grade = grade_from_score(class_score, profile);
    let needs_refactoring = class_score < refactoring_threshold;
    let method_count = method_reports.len() as u32;
    let total_loc: u32 = method_reports
        .iter()
        .map(|r| r.metrics.structural.loc)
        .sum();
    let max_nesting = method_reports
        .iter()
        .map(|r| r.metrics.structural.max_nesting_depth)
        .max()
        .unwrap_or(0);

    let structural = StructuralResult {
        loc: total_loc,
        total_lines: 0,
        parameter_count: 0,
        max_nesting_depth: max_nesting,
        return_count: 0,
        method_count: Some(method_count),
        raw_score: compute_sm_raw(total_loc, 0, max_nesting, 0),
    };

    ClassQualityReport {
        name: ce.name,
        score: class_score,
        grade,
        needs_refactoring,
        flags: vec![],
        structural_metrics: structural,
        methods: method_reports,
        location: SourceLocation {
            file: String::new(),
            start_line: ce.start_line,
            end_line: ce.end_line,
            start_col: 0,
            end_col: 0,
        },
    }
}
