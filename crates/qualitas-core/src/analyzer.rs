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
use crate::types::{
    AnalysisOptions, ClassQualityReport, FileQualityReport, FunctionQualityReport, MetricBreakdown,
    SourceLocation, StructuralResult, WeightConfig,
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
    let (function_reports, class_reports) = build_reports(extraction, source, &ctx);

    Ok(assemble_file_report(
        file_path,
        source,
        &ctx,
        function_reports,
        class_reports,
        file_deps,
    ))
}

struct AnalysisContext<'a> {
    profile: Option<&'a str>,
    weights: Option<&'a WeightConfig>,
    threshold: f64,
}

impl<'a> AnalysisContext<'a> {
    fn from_options(options: &'a AnalysisOptions) -> Self {
        Self {
            profile: options.profile.as_deref(),
            weights: options.weights.as_ref(),
            threshold: options
                .refactoring_threshold
                .unwrap_or(crate::constants::DEFAULT_REFACTORING_THRESHOLD),
        }
    }
}

fn build_reports(
    extraction: FileExtraction,
    source: &str,
    ctx: &AnalysisContext<'_>,
) -> (Vec<FunctionQualityReport>, Vec<ClassQualityReport>) {
    let FileExtraction {
        functions,
        classes,
        imports,
    } = extraction;

    let function_reports = functions
        .into_iter()
        .map(|fe| build_fn_report_from_events(fe, source, &imports, ctx))
        .collect();

    let class_reports = classes
        .into_iter()
        .map(|ce| build_class_report(ce, source, &imports, ctx))
        .collect();

    (function_reports, class_reports)
}

fn assemble_file_report(
    file_path: &str,
    source: &str,
    ctx: &AnalysisContext<'_>,
    function_reports: Vec<FunctionQualityReport>,
    class_reports: Vec<ClassQualityReport>,
    file_deps: crate::types::DependencyCouplingResult,
) -> FileQualityReport {
    let file_score = compute_file_score(&function_reports, &class_reports);
    let fn_count = count_total_functions(&function_reports, &class_reports);
    let class_count = class_reports.len() as u32;
    let flagged = count_flagged(&function_reports);

    FileQualityReport {
        file_path: file_path.to_string(),
        score: file_score,
        grade: grade_from_score(file_score, ctx.profile),
        needs_refactoring: file_score < ctx.threshold,
        flags: vec![],
        functions: function_reports,
        classes: class_reports,
        file_dependencies: file_deps,
        total_lines: source.lines().count() as u32,
        function_count: fn_count,
        class_count,
        flagged_function_count: flagged,
    }
}

fn count_total_functions(fns: &[FunctionQualityReport], classes: &[ClassQualityReport]) -> u32 {
    fns.len() as u32 + classes.iter().map(|c| c.methods.len() as u32).sum::<u32>()
}

fn count_flagged(reports: &[FunctionQualityReport]) -> u32 {
    reports.iter().filter(|r| r.needs_refactoring).count() as u32
}

/// LOC-weighted average score across all functions and class methods.
fn compute_file_score(
    function_reports: &[FunctionQualityReport],
    class_reports: &[ClassQualityReport],
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
    if scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&scores)
    }
}

// ─── Report assembly from event streams ─────────────────────────────────────

fn build_fn_report_from_events(
    fe: FunctionExtraction,
    source: &str,
    imports: &[crate::ir::language::ImportRecord],
    ctx: &AnalysisContext<'_>,
) -> FunctionQualityReport {
    let cfc = compute_cfc(&fe.events);
    let dci = compute_dci(&fe.events);
    let irc = compute_irc(&fe.events, source);
    let dc = compute_dc_from_events(&fe.events, imports);
    let sm = compute_sm_from_events(
        &fe.events,
        source,
        fe.byte_start,
        fe.byte_end,
        fe.param_count,
    );

    let metrics = MetricBreakdown {
        cognitive_flow: cfc,
        data_complexity: dci,
        identifier_reference: irc,
        dependency_coupling: dc,
        structural: sm,
    };

    let (score, breakdown) = compute_score(&metrics, ctx.weights, ctx.profile);
    let grade = grade_from_score(score, ctx.profile);
    let needs_refactoring = score < ctx.threshold;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AnalysisOptions, Grade};

    fn default_options() -> AnalysisOptions {
        AnalysisOptions::default()
    }

    #[test]
    fn analyze_clean_ts_returns_high_score() {
        let source = r"
function add(a: number, b: number): number {
    return a + b;
}
";
        let report = analyze_source_str(source, "clean.ts", &default_options()).unwrap();
        assert!(
            report.score >= 80.0,
            "Expected score >= 80 for clean function, got {:.2}",
            report.score,
        );
        assert_eq!(report.grade, Grade::A);
    }

    #[test]
    fn analyze_complex_ts_returns_low_score() {
        let source = r"
function processOrders(orders: any[], config: any, logger: any, db: any, cache: any, validator: any) {
    const results: any[] = [];
    for (const order of orders) {
        if (order.status === 'pending') {
            if (order.items && order.items.length > 0) {
                for (const item of order.items) {
                    if (item.quantity > 0) {
                        try {
                            if (validator.isValid(item)) {
                                if (config.dryRun || config.verbose && logger.level === 'debug') {
                                    logger.info('processing');
                                }
                                const price = item.price * item.quantity;
                                if (price > config.maxPrice) {
                                    results.push({ status: 'skipped', reason: 'too expensive' });
                                } else {
                                    results.push({ status: 'processed', price: price });
                                }
                            }
                        } catch (err: any) {
                            if (err.code === 'NETWORK') {
                                logger.error(err.message);
                                cache.invalidate(order.id);
                            } else {
                                db.log(err);
                            }
                        }
                    }
                }
            }
        }
    }
    return results;
}
";
        let report = analyze_source_str(source, "complex.ts", &default_options()).unwrap();
        assert!(
            report.score < 65.0,
            "Expected score < 65 for complex function, got {:.2}",
            report.score,
        );
        assert!(
            report.grade == Grade::C || report.grade == Grade::D || report.grade == Grade::F,
            "Expected grade C, D, or F for complex function, got {:?}",
            report.grade,
        );
    }

    #[test]
    fn analyze_empty_file_returns_perfect() {
        let report = analyze_source_str("", "empty.ts", &default_options()).unwrap();
        assert!(
            (report.score - 100.0).abs() < 0.01,
            "Expected score 100 for empty file, got {:.2}",
            report.score,
        );
    }

    #[test]
    fn analyze_class_aggregates_methods() {
        let source = r"
class Calculator {
    add(a: number, b: number) { return a + b; }
    subtract(a: number, b: number) { return a - b; }
}
";
        let report = analyze_source_str(source, "class.ts", &default_options()).unwrap();
        assert_eq!(
            report.class_count, 1,
            "Expected 1 class, got {}",
            report.class_count,
        );
        assert_eq!(
            report.function_count, 2,
            "Expected function_count=2 (methods counted in total), got {}",
            report.function_count,
        );
        // Top-level functions list should be empty — methods live inside the class
        assert!(
            report.functions.is_empty(),
            "Expected no top-level functions, got {}",
            report.functions.len(),
        );
        assert_eq!(
            report.classes.len(),
            1,
            "Expected 1 class report, got {}",
            report.classes.len(),
        );
        assert_eq!(
            report.classes[0].methods.len(),
            2,
            "Expected 2 methods in class, got {}",
            report.classes[0].methods.len(),
        );
    }

    #[test]
    fn analyze_file_score_is_loc_weighted() {
        // A short clean function and a longer messier function.
        // The file score should be pulled toward the longer function's score.
        let source = r"
function tiny(a: number): number { return a; }

function longer(x: number): number {
    let result = 0;
    if (x > 0) {
        if (x > 10) {
            if (x > 100) {
                result = x * 2;
            } else {
                result = x + 1;
            }
        } else {
            result = x - 1;
        }
    } else {
        result = -x;
    }
    return result;
}
";
        let report = analyze_source_str(source, "weighted.ts", &default_options()).unwrap();
        assert_eq!(report.functions.len(), 2);
        let tiny_score = report.functions[0].score;
        let longer_score = report.functions[1].score;
        // The longer function should score lower (more complexity)
        assert!(
            longer_score < tiny_score,
            "Expected longer function ({:.2}) to score lower than tiny ({:.2})",
            longer_score,
            tiny_score,
        );
        // File score should be closer to the longer function's score due to LOC weighting
        let simple_avg = (tiny_score + longer_score) / 2.0;
        // LOC-weighted average pulls toward the longer function
        // file_score should be <= simple_avg (closer to the worse function)
        assert!(
            report.score <= simple_avg + 1.0,
            "Expected file score ({:.2}) to be at or below simple average ({:.2}) due to LOC weighting",
            report.score,
            simple_avg,
        );
    }

    #[test]
    fn analyze_rust_source_works() {
        let source = r"
fn add(a: i32, b: i32) -> i32 {
    a + b
}
";
        let report = analyze_source_str(source, "simple.rs", &default_options()).unwrap();
        assert!(
            report.score >= 80.0,
            "Expected score >= 80 for simple Rust function, got {:.2}",
            report.score,
        );
        assert_eq!(report.grade, Grade::A);
        assert_eq!(report.function_count, 1);
        assert!(!report.needs_refactoring);
    }
}
