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
    structural::{compute_sm_from_events, compute_sm_raw, compute_sm_with_loc, SourceSpan},
};
use crate::scorer::{
    composite::{aggregate_scores, compute_score},
    thresholds::{generate_flags, grade_from_score},
};
use crate::types::{
    AnalysisOptions, ClassQualityReport, FileQualityReport, FlagConfig, FunctionQualityReport,
    MetricBreakdown, SourceLocation, StructuralResult, WeightConfig,
};

pub fn analyze_source_str(
    source: &str,
    file_path: &str,
    options: &AnalysisOptions,
) -> Result<FileQualityReport, String> {
    let adapter = adapter_for_file(file_path)?;
    let extraction = adapter.extract(source, file_path)?;
    let ctx = AnalysisContext::from_options(options);

    let file_deps = analyze_file_dependencies_ir(&extraction.imports);
    let (function_reports, class_reports, file_scope) = build_reports(extraction, source, &ctx);

    Ok(assemble_file_report(FileReportParts {
        file_path,
        source,
        ctx: &ctx,
        function_reports,
        class_reports,
        file_deps,
        file_scope,
    }))
}

struct FileReportParts<'a> {
    file_path: &'a str,
    source: &'a str,
    ctx: &'a AnalysisContext<'a>,
    function_reports: Vec<FunctionQualityReport>,
    class_reports: Vec<ClassQualityReport>,
    file_deps: crate::types::DependencyCouplingResult,
    file_scope: Option<Box<FunctionQualityReport>>,
}

struct AnalysisContext<'a> {
    profile: Option<&'a str>,
    weights: Option<&'a WeightConfig>,
    threshold: f64,
    flag_overrides: Option<&'a std::collections::HashMap<String, FlagConfig>>,
}

impl<'a> AnalysisContext<'a> {
    fn from_options(options: &'a AnalysisOptions) -> Self {
        Self {
            profile: options.profile.as_deref(),
            weights: options.weights.as_ref(),
            threshold: options
                .refactoring_threshold
                .unwrap_or(crate::constants::DEFAULT_REFACTORING_THRESHOLD),
            flag_overrides: options.flag_overrides.as_ref(),
        }
    }
}

fn build_reports(
    extraction: FileExtraction,
    source: &str,
    ctx: &AnalysisContext<'_>,
) -> (
    Vec<FunctionQualityReport>,
    Vec<ClassQualityReport>,
    Option<Box<FunctionQualityReport>>,
) {
    let FileExtraction {
        functions,
        classes,
        imports,
        file_scope,
    } = extraction;

    let function_reports = functions
        .into_iter()
        .map(|fe| build_fn_report_from_events(fe, source, &imports, ctx))
        .collect();

    let class_reports = classes
        .into_iter()
        .map(|ce| build_class_report(ce, source, &imports, ctx))
        .collect();

    let file_scope_report =
        file_scope.map(|fs| Box::new(build_fn_report_from_events(fs, source, &imports, ctx)));

    (function_reports, class_reports, file_scope_report)
}

fn assemble_file_report(parts: FileReportParts<'_>) -> FileQualityReport {
    let file_score = compute_file_score(
        &parts.function_reports,
        &parts.class_reports,
        parts.file_scope.as_deref(),
    );
    let fn_count = count_total_functions(&parts.function_reports, &parts.class_reports);
    let class_count = parts.class_reports.len() as u32;
    let flagged = count_flagged(&parts.function_reports);

    FileQualityReport {
        file_path: parts.file_path.to_string(),
        score: file_score,
        grade: grade_from_score(file_score, parts.ctx.profile),
        needs_refactoring: file_score < parts.ctx.threshold,
        flags: vec![],
        functions: parts.function_reports,
        classes: parts.class_reports,
        file_dependencies: parts.file_deps,
        total_lines: parts.source.lines().count() as u32,
        function_count: fn_count,
        class_count,
        flagged_function_count: flagged,
        file_scope: parts.file_scope,
    }
}

fn count_total_functions(fns: &[FunctionQualityReport], classes: &[ClassQualityReport]) -> u32 {
    fns.len() as u32 + classes.iter().map(|c| c.methods.len() as u32).sum::<u32>()
}

fn count_flagged(reports: &[FunctionQualityReport]) -> u32 {
    reports.iter().filter(|r| r.needs_refactoring).count() as u32
}

/// LOC-weighted average score across all functions, class methods, and file-scope.
fn compute_file_score(
    function_reports: &[FunctionQualityReport],
    class_reports: &[ClassQualityReport],
    file_scope: Option<&FunctionQualityReport>,
) -> f64 {
    let mut scores: Vec<(f64, u32)> = function_reports
        .iter()
        .map(|r| (r.score, r.metrics.structural.loc.max(1)))
        .collect();
    for cr in class_reports {
        for mr in &cr.methods {
            scores.push((mr.score, mr.metrics.structural.loc.max(1)));
        }
    }
    if let Some(fs) = file_scope {
        scores.push((fs.score, fs.metrics.structural.loc.max(1)));
    }
    if scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&scores)
    }
}

// ─── Report assembly from event streams ─────────────────────────────────────

fn collect_metrics(
    fe: &FunctionExtraction,
    source: &str,
    imports: &[crate::ir::language::ImportRecord],
) -> MetricBreakdown {
    let mut sm = if let Some(loc) = fe.loc_override {
        compute_sm_with_loc(&fe.events, loc, loc, fe.param_count)
    } else {
        compute_sm_from_events(
            &fe.events,
            &SourceSpan {
                source,
                start: fe.byte_start,
                end: fe.byte_end,
            },
            fe.param_count,
        )
    };
    // Use logical LOC (statement count) when available — avoids penalizing
    // code formatters that expand single statements across multiple lines.
    if let Some(stmt_count) = fe.statement_count {
        sm.loc = stmt_count;
        sm.raw_score = compute_sm_raw(
            stmt_count,
            sm.parameter_count,
            sm.max_nesting_depth,
            sm.return_count,
        );
    }
    MetricBreakdown {
        cognitive_flow: compute_cfc(&fe.events),
        data_complexity: compute_dci(&fe.events),
        identifier_reference: compute_irc(&fe.events, source),
        dependency_coupling: compute_dc_from_events(&fe.events, imports),
        structural: sm,
    }
}

fn build_fn_report_from_events(
    fe: FunctionExtraction,
    source: &str,
    imports: &[crate::ir::language::ImportRecord],
    ctx: &AnalysisContext<'_>,
) -> FunctionQualityReport {
    let metrics = collect_metrics(&fe, source, imports);
    let (score, breakdown) = compute_score(&metrics, ctx.weights, ctx.profile);

    FunctionQualityReport {
        name: fe.name,
        inferred_name: fe.inferred_name,
        score,
        grade: grade_from_score(score, ctx.profile),
        needs_refactoring: score < ctx.threshold,
        flags: generate_flags(&metrics, ctx.flag_overrides),
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

fn build_class_report(
    ce: ClassExtraction,
    source: &str,
    imports: &[crate::ir::language::ImportRecord],
    ctx: &AnalysisContext<'_>,
) -> ClassQualityReport {
    let method_reports: Vec<FunctionQualityReport> = ce
        .methods
        .into_iter()
        .map(|m| build_fn_report_from_events(m, source, imports, ctx))
        .collect();

    let class_score = compute_class_score(&method_reports);
    let structural = aggregate_class_structural(&method_reports);

    ClassQualityReport {
        name: ce.name,
        score: class_score,
        grade: grade_from_score(class_score, ctx.profile),
        needs_refactoring: class_score < ctx.threshold,
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

fn compute_class_score(methods: &[FunctionQualityReport]) -> f64 {
    let scores: Vec<(f64, u32)> = methods
        .iter()
        .map(|r| (r.score, r.metrics.structural.loc.max(1)))
        .collect();
    if scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&scores)
    }
}

fn aggregate_class_structural(methods: &[FunctionQualityReport]) -> StructuralResult {
    let method_count = methods.len() as u32;
    let total_loc: u32 = methods.iter().map(|r| r.metrics.structural.loc).sum();
    let max_nesting = methods
        .iter()
        .map(|r| r.metrics.structural.max_nesting_depth)
        .max()
        .unwrap_or(0);

    StructuralResult {
        loc: total_loc,
        total_lines: 0,
        parameter_count: 0,
        max_nesting_depth: max_nesting,
        return_count: 0,
        method_count: Some(method_count),
        raw_score: compute_sm_raw(total_loc, 0, max_nesting, 0),
    }
}
