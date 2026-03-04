mod config;
mod reporters;

use std::path::Path;
use std::process;

use clap::Parser;
use walkdir::WalkDir;

use qualitas_core::analyzer::analyze_source_str;
use qualitas_core::languages::list_adapters;
use qualitas_core::scorer::composite::aggregate_scores;
use qualitas_core::scorer::thresholds::grade_from_score;
use qualitas_core::types::{
    AnalysisOptions, FileQualityReport, FunctionQualityReport, GradeDistribution,
    ProjectQualityReport, ProjectSummary,
};

use reporters::json::{render_file_json, render_project_json};
use reporters::markdown::{render_markdown_project_report, render_markdown_report};
use reporters::summary::render_executive_summary;
use reporters::text::{render_file_report, render_project_report, TextReporterOptions};

// ─── CLI argument parsing ─────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "qualitas",
    about = "Code quality measurement \u{2014} Quality Score 0\u{2013}100 (higher = better)",
    version
)]
struct Cli {
    /// File or directory to analyze
    path: String,

    /// Output format: text | json | markdown
    #[arg(short = 'f', long)]
    format: Option<String>,

    /// Weight profile: default | cc-focused | data-focused | strict
    #[arg(short = 'p', long)]
    profile: Option<String>,

    /// Exit code 1 if any score is below this threshold
    #[arg(short = 't', long)]
    threshold: Option<f64>,

    /// Only show items needing refactoring
    #[arg(long)]
    flagged_only: bool,

    /// Show metric breakdown per function
    #[arg(long)]
    verbose: bool,

    /// Report scope: function | class | file | module
    #[arg(long)]
    scope: Option<String>,

    /// Include test files (*.test.*, *.spec.*) in analysis
    #[arg(long)]
    include_tests: bool,
}

// ─── Default file-collection settings ─────────────────────────────────────────

const DEFAULT_EXCLUDE: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".git",
    "coverage",
    "target",
];

// ─── Extracted helper: handle analysis result and exit ─────────────────────────

fn handle_result(result: Result<bool, String>) -> ! {
    match result {
        Ok(below_threshold) => process::exit(i32::from(below_threshold)),
        Err(e) => {
            eprintln!("qualitas error: {e}");
            process::exit(2);
        }
    }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    let config = config::load_config(&cli.path);
    let (options, reporter_opts, format) = config::merge_config(&cli, &config);

    let target = Path::new(&cli.path);

    if !target.exists() {
        eprintln!("qualitas: path not found: {}", cli.path);
        process::exit(2);
    }

    let result = if target.is_dir() {
        run_project(&cli, &options, &reporter_opts, &format)
    } else {
        run_file(&cli.path, &options, &reporter_opts, &format)
    };

    handle_result(result);
}

// ─── Single-file analysis ─────────────────────────────────────────────────────

fn run_file(
    path: &str,
    options: &AnalysisOptions,
    reporter_opts: &TextReporterOptions,
    format: &str,
) -> Result<bool, String> {
    let report = analyze_file(path, options)?;
    let threshold = options.refactoring_threshold.unwrap_or(65.0);
    let below = report.score < threshold || report.functions.iter().any(|f| f.score < threshold);

    println!("{}", format_file_output(&report, format, reporter_opts));
    Ok(below)
}

fn format_file_output(
    report: &FileQualityReport,
    format: &str,
    reporter_opts: &TextReporterOptions,
) -> String {
    match format {
        "json" => render_file_json(report),
        "markdown" => render_markdown_report(report),
        _ => render_file_report(report, reporter_opts),
    }
}

// ─── Extracted helper: analyze all files, skipping errors ─────────────────────

fn analyze_all_files(files: &[String], options: &AnalysisOptions) -> Vec<FileQualityReport> {
    let mut file_reports = Vec::new();
    for file_path in files {
        match analyze_file(file_path, options) {
            Ok(report) => file_reports.push(report),
            Err(e) => {
                eprintln!("qualitas: skipping {file_path}: {e}");
            }
        }
    }
    file_reports
}

// ─── Extracted helper: check if project score is below threshold ──────────────

fn check_project_threshold(report: &ProjectQualityReport, threshold: f64) -> bool {
    report.score < threshold
        || report
            .files
            .iter()
            .any(|f| f.functions.iter().any(|func| func.score < threshold))
}

// ─── Extracted helper: format project output ──────────────────────────────────

fn format_project_output(
    report: &ProjectQualityReport,
    format: &str,
    reporter_opts: &TextReporterOptions,
) -> String {
    match format {
        "json" => render_project_json(report),
        "markdown" => render_markdown_project_report(report),
        "summary" => render_executive_summary(report),
        _ => render_project_report(report, reporter_opts),
    }
}

// ─── Project analysis ─────────────────────────────────────────────────────────

fn run_project(
    cli: &Cli,
    options: &AnalysisOptions,
    reporter_opts: &TextReporterOptions,
    format: &str,
) -> Result<bool, String> {
    let include_tests = options.include_tests.unwrap_or(false);
    let threshold = options.refactoring_threshold.unwrap_or(65.0);

    let files = collect_files(&cli.path, include_tests)?;

    if files.is_empty() {
        eprintln!("qualitas: no supported files found in {}", cli.path);
        process::exit(2);
    }

    let file_reports = analyze_all_files(&files, options);

    let report = build_project_report(&cli.path, file_reports, threshold);

    let below_threshold = check_project_threshold(&report, threshold);

    println!("{}", format_project_output(&report, format, reporter_opts));
    Ok(below_threshold)
}

// ─── File analysis helper ─────────────────────────────────────────────────────

fn analyze_file(file_path: &str, options: &AnalysisOptions) -> Result<FileQualityReport, String> {
    let source =
        std::fs::read_to_string(file_path).map_err(|e| format!("cannot read {file_path}: {e}"))?;

    let mut report = analyze_source_str(&source, file_path, options)?;

    // Backfill file path into location objects
    for func in &mut report.functions {
        func.location.file = file_path.to_string();
    }
    for cls in &mut report.classes {
        cls.location.file = file_path.to_string();
        for m in &mut cls.methods {
            m.location.file = file_path.to_string();
        }
    }

    Ok(report)
}

// ─── Extracted helper: check if a single file is supported ────────────────────

fn is_supported_file(
    path: &Path,
    supported_extensions: &[&str],
    test_patterns: &[&str],
    include_tests: bool,
) -> bool {
    let full_path = path.to_string_lossy();
    let name = path.file_name().unwrap_or_default().to_string_lossy();

    // Check extension
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    if !supported_extensions.contains(&ext.as_str()) {
        return false;
    }

    // Skip test files unless requested (check both file name and path components)
    if !include_tests
        && test_patterns
            .iter()
            .any(|p| name.contains(p) || full_path.contains(p))
    {
        return false;
    }

    true
}

// ─── Extracted helper: load adapter extensions and test patterns ──────────────

fn load_adapter_info() -> (Vec<&'static str>, Vec<&'static str>) {
    let adapters = list_adapters();
    let extensions: Vec<&str> = adapters
        .iter()
        .flat_map(|a| a.extensions().iter().copied())
        .collect();
    let test_patterns: Vec<&str> = adapters
        .iter()
        .flat_map(|a| a.test_patterns().iter().copied())
        .collect();
    (extensions, test_patterns)
}

// ─── File collection (walkdir) ────────────────────────────────────────────────

/// Returns true if a directory entry should be descended into (not hidden, not excluded).
fn should_enter_directory(entry: &walkdir::DirEntry) -> bool {
    // Always enter the root directory (depth 0), even if it's "." or "./"
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    if entry.file_type().is_dir() {
        return !name.starts_with('.') && !DEFAULT_EXCLUDE.contains(&name.as_ref());
    }
    true
}

fn collect_files(dir: &str, include_tests: bool) -> Result<Vec<String>, String> {
    let (extensions, patterns) = load_adapter_info();
    let mut files = collect_matching_files(dir, &extensions, &patterns, include_tests)?;
    files.sort();
    Ok(files)
}

fn collect_matching_files(
    dir: &str,
    extensions: &[&str],
    patterns: &[&str],
    include_tests: bool,
) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_entry(should_enter_directory)
    {
        let entry = entry.map_err(|e| format!("walkdir error: {e}"))?;
        if entry.file_type().is_file()
            && is_supported_file(entry.path(), extensions, patterns, include_tests)
        {
            files.push(entry.path().to_string_lossy().to_string());
        }
    }
    Ok(files)
}

// ─── Extracted helper: collect all functions from file reports ─────────────────

fn collect_all_functions(reports: &[FileQualityReport]) -> Vec<&FunctionQualityReport> {
    let mut all_functions: Vec<&FunctionQualityReport> = Vec::new();
    for fr in reports {
        for func in &fr.functions {
            all_functions.push(func);
        }
        for cls in &fr.classes {
            for m in &cls.methods {
                all_functions.push(m);
            }
        }
    }
    all_functions
}

// ─── Extracted helper: build grade distribution ───────────────────────────────

fn build_grade_distribution(functions: &[&FunctionQualityReport]) -> GradeDistribution {
    let mut counts = [0u32; 5];
    for func in functions {
        counts[func.grade.index()] += 1;
    }
    GradeDistribution {
        a: counts[0],
        b: counts[1],
        c: counts[2],
        d: counts[3],
        f: counts[4],
    }
}

// ─── Extracted helper: find worst-scoring functions ───────────────────────────

fn find_worst_functions(functions: &[&FunctionQualityReport]) -> Vec<FunctionQualityReport> {
    let mut worst: Vec<FunctionQualityReport> = functions.iter().map(|f| (*f).clone()).collect();
    worst.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    worst
}

// ─── Project report builder ───────────────────────────────────────────────────

fn build_project_report(
    dir_path: &str,
    file_reports: Vec<FileQualityReport>,
    threshold: f64,
) -> ProjectQualityReport {
    let all_functions = collect_all_functions(&file_reports);

    // LOC-weighted average
    let scores: Vec<(f64, u32)> = all_functions
        .iter()
        .map(|f| (f.score, f.metrics.structural.loc.max(1)))
        .collect();

    let weighted_score = if scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&scores)
    };

    let dist = build_grade_distribution(&all_functions);

    let worst = find_worst_functions(&all_functions);

    let grade = grade_from_score(weighted_score, None);

    ProjectQualityReport {
        dir_path: dir_path.to_string(),
        score: weighted_score,
        grade,
        needs_refactoring: weighted_score < threshold,
        summary: ProjectSummary {
            total_files: file_reports.len() as u32,
            total_functions: all_functions.len() as u32,
            total_classes: file_reports.iter().map(|f| f.class_count).sum(),
            flagged_files: file_reports.iter().filter(|f| f.needs_refactoring).count() as u32,
            flagged_functions: all_functions.iter().filter(|f| f.needs_refactoring).count() as u32,
            average_score: weighted_score,
            grade_distribution: dist,
        },
        files: file_reports,
        worst_functions: worst,
    }
}
