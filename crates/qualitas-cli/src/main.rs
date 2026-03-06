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
    AnalysisOptions, FileQualityReport, FlagConfig, FunctionQualityReport, GradeDistribution,
    ProjectQualityReport, ProjectSummary, QualitasConfig,
};

use reporters::compact::{render_compact_file, render_compact_project};
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

    /// Output format: text | compact | detail | flagged | json | markdown | summary
    #[arg(short = 'f', long)]
    format: Option<String>,

    /// Weight profile: default | cc-focused | data-focused | strict
    #[arg(short = 'p', long)]
    profile: Option<String>,

    /// Exit code 1 if any score is below this threshold
    #[arg(short = 't', long)]
    threshold: Option<f64>,

    /// Include test files (*.test.*, *.spec.*) in analysis
    #[arg(long)]
    include_tests: bool,

    /// Fail (exit 1) if any function has flags at this severity: warn | error
    #[arg(long)]
    fail_on_flags: Option<String>,
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

// ─── Format → TextReporterOptions mapping ─────────────────────────────────────

fn reporter_opts_for_format(format: &str) -> TextReporterOptions {
    match format {
        "detail" => TextReporterOptions {
            verbose: true,
            flagged_only: false,
            scope: "function".to_string(),
        },
        "flagged" => TextReporterOptions {
            verbose: false,
            flagged_only: true,
            scope: "function".to_string(),
        },
        _ => TextReporterOptions::default(),
    }
}

// ─── Extracted helper: handle analysis result and exit ─────────────────────────

fn validate_path(path: &str) {
    if !Path::new(path).exists() {
        eprintln!("qualitas: path not found: {path}");
        process::exit(2);
    }
}

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
    let (options, format) = config::merge_config(&cli, &config);

    let fail_on_flags = cli
        .fail_on_flags
        .as_deref()
        .or(config.fail_on_flags.as_deref());

    validate_path(&cli.path);

    let result = if Path::new(&cli.path).is_dir() {
        run_project(&cli, &options, &format, &config, fail_on_flags)
    } else {
        let mut opts = options.clone();
        opts.flag_overrides = resolve_flag_overrides(&cli.path, &config);
        run_file(&cli.path, &opts, &format, fail_on_flags)
    };

    handle_result(result);
}

// ─── Single-file analysis ─────────────────────────────────────────────────────

fn run_file(
    path: &str,
    options: &AnalysisOptions,
    format: &str,
    fail_on_flags: Option<&str>,
) -> Result<bool, String> {
    let report = analyze_file(path, options)?;
    let threshold = options.refactoring_threshold.unwrap_or(65.0);
    let below = report.score < threshold
        || report.functions.iter().any(|f| f.score < threshold)
        || report
            .file_scope
            .as_ref()
            .is_some_and(|fs| fs.score < threshold)
        || has_flags_at_severity(&report.functions, report.file_scope.as_deref(), fail_on_flags);

    println!("{}", format_file_output(&report, format));
    Ok(below)
}

fn format_file_output(report: &FileQualityReport, format: &str) -> String {
    match format {
        "json" => render_file_json(report),
        "markdown" => render_markdown_report(report),
        "compact" => render_compact_file(report),
        _ => {
            let opts = reporter_opts_for_format(format);
            render_file_report(report, &opts)
        }
    }
}

// ─── Extracted helper: analyze all files, skipping errors ─────────────────────

fn analyze_all_files(
    files: &[String],
    options: &AnalysisOptions,
    config: &QualitasConfig,
) -> Vec<FileQualityReport> {
    let mut file_reports = Vec::new();
    for file_path in files {
        let mut opts = options.clone();
        opts.flag_overrides = resolve_flag_overrides(file_path, config);
        match analyze_file(file_path, &opts) {
            Ok(report) => file_reports.push(report),
            Err(e) => {
                eprintln!("qualitas: skipping {file_path}: {e}");
            }
        }
    }
    file_reports
}

// ─── Extracted helper: check if project score is below threshold ──────────────

fn check_project_threshold(
    report: &ProjectQualityReport,
    threshold: f64,
    fail_on_flags: Option<&str>,
) -> bool {
    report.score < threshold
        || report.files.iter().any(|f| {
            f.functions.iter().any(|func| func.score < threshold)
                || f.file_scope.as_ref().is_some_and(|fs| fs.score < threshold)
                || has_flags_at_severity(&f.functions, f.file_scope.as_deref(), fail_on_flags)
        })
}

/// Check if any function has flags at the specified severity level.
fn has_flags_at_severity(
    functions: &[FunctionQualityReport],
    file_scope: Option<&FunctionQualityReport>,
    fail_on_flags: Option<&str>,
) -> bool {
    let Some(level) = fail_on_flags else {
        return false;
    };
    let check_fn = |f: &FunctionQualityReport| match level {
        "warn" => !f.flags.is_empty(),
        "error" => f.flags.iter().any(|flag| {
            flag.severity == qualitas_core::types::Severity::Error
        }),
        _ => false,
    };
    functions.iter().any(check_fn)
        || file_scope.is_some_and(check_fn)
}

// ─── Extracted helper: format project output ──────────────────────────────────

fn format_project_output(report: &ProjectQualityReport, format: &str) -> String {
    match format {
        "json" => render_project_json(report),
        "markdown" => render_markdown_project_report(report),
        "summary" => render_executive_summary(report),
        "compact" => render_compact_project(report),
        _ => {
            let opts = reporter_opts_for_format(format);
            render_project_report(report, &opts)
        }
    }
}

// ─── Project analysis ─────────────────────────────────────────────────────────

fn run_project(
    cli: &Cli,
    options: &AnalysisOptions,
    format: &str,
    config: &qualitas_core::types::QualitasConfig,
    fail_on_flags: Option<&str>,
) -> Result<bool, String> {
    let include_tests = options.include_tests.unwrap_or(false);
    let threshold = options.refactoring_threshold.unwrap_or(65.0);

    let files = collect_files(&cli.path, include_tests, config)?;

    if files.is_empty() {
        eprintln!("qualitas: no supported files found in {}", cli.path);
        process::exit(2);
    }

    let file_reports = analyze_all_files(&files, options, config);

    let report = build_project_report(&cli.path, file_reports, threshold);

    let below_threshold = check_project_threshold(&report, threshold, fail_on_flags);

    println!("{}", format_project_output(&report, format));
    Ok(below_threshold)
}

// ─── File analysis helper ─────────────────────────────────────────────────────

fn analyze_file(file_path: &str, options: &AnalysisOptions) -> Result<FileQualityReport, String> {
    let source =
        std::fs::read_to_string(file_path).map_err(|e| format!("cannot read {file_path}: {e}"))?;

    let mut report = analyze_source_str(&source, file_path, options)?;

    // Backfill file path into location objects
    if let Some(fs) = &mut report.file_scope {
        fs.location.file = file_path.to_string();
    }
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
    supported_extensions: &[String],
    ext_patterns: &ExtTestPatterns,
    include_tests: bool,
) -> bool {
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    if !supported_extensions.iter().any(|e| e == &ext) {
        return false;
    }

    if !include_tests && matches_test_pattern(path, ext_patterns.get(&ext)) {
        return false;
    }

    true
}

fn matches_test_pattern(path: &Path, patterns: Option<&Vec<String>>) -> bool {
    let Some(patterns) = patterns else {
        return false;
    };
    let full_path = path.to_string_lossy();
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    patterns
        .iter()
        .any(|p| name.contains(p.as_str()) || full_path.contains(p.as_str()))
}

// ─── Extracted helper: load adapter extensions and test patterns ──────────────

/// Map of file extension → test patterns for that language.
type ExtTestPatterns = std::collections::HashMap<String, Vec<String>>;

fn load_filter_info(
    config: &qualitas_core::types::QualitasConfig,
) -> (Vec<String>, ExtTestPatterns, Vec<String>) {
    let adapters = list_adapters();
    let mut extensions = Vec::new();
    let mut ext_patterns: ExtTestPatterns = std::collections::HashMap::new();

    for adapter in adapters {
        let patterns = resolve_adapter_patterns(adapter.as_ref(), config);
        for ext in adapter.extensions() {
            extensions.push((*ext).to_string());
            ext_patterns.insert((*ext).to_string(), patterns.clone());
        }
    }

    let excludes = resolve_excludes(config);

    (extensions, ext_patterns, excludes)
}

fn resolve_adapter_patterns(
    adapter: &dyn qualitas_core::ir::language::LanguageAdapter,
    config: &qualitas_core::types::QualitasConfig,
) -> Vec<String> {
    find_language_test_patterns(adapter.name(), config).unwrap_or_else(|| {
        adapter
            .test_patterns()
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    })
}

fn find_language_test_patterns(
    adapter_name: &str,
    config: &qualitas_core::types::QualitasConfig,
) -> Option<Vec<String>> {
    let langs = config.languages.as_ref()?;
    let key = adapter_name.to_lowercase();
    let lang_cfg = langs.get(&key).or_else(|| {
        langs
            .iter()
            .find(|(k, _)| key.contains(k.as_str()))
            .map(|(_, v)| v)
    })?;
    lang_cfg.test_patterns.clone()
}

fn resolve_excludes(config: &qualitas_core::types::QualitasConfig) -> Vec<String> {
    if let Some(user_excludes) = &config.exclude {
        user_excludes.clone()
    } else {
        DEFAULT_EXCLUDE.iter().map(|s| (*s).to_string()).collect()
    }
}

// ─── Per-file flag config resolution ──────────────────────────────────────────

fn language_for_extension(file_path: &str) -> Option<String> {
    let ext = Path::new(file_path).extension()?.to_str()?;
    match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => Some("typescript".to_string()),
        "rs" => Some("rust".to_string()),
        _ => None,
    }
}

fn resolve_flag_overrides(
    file_path: &str,
    config: &QualitasConfig,
) -> Option<std::collections::HashMap<String, FlagConfig>> {
    let global = config.flags.clone();
    let lang = language_for_extension(file_path)
        .and_then(|l| config.languages.as_ref()?.get(&l)?.flags.clone());

    match (global, lang) {
        (None, None) => None,
        (Some(g), None) | (None, Some(g)) => Some(g),
        (Some(mut g), Some(l)) => {
            g.extend(l);
            Some(g)
        }
    }
}

// ─── File collection (walkdir) ────────────────────────────────────────────────

fn collect_files(
    dir: &str,
    include_tests: bool,
    config: &qualitas_core::types::QualitasConfig,
) -> Result<Vec<String>, String> {
    let (extensions, test_patterns, excludes) = load_filter_info(config);
    let mut files = Vec::new();
    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_entry(|e| should_include_entry(e, &excludes))
    {
        let entry = entry.map_err(|e| format!("walkdir error: {e}"))?;
        if entry.file_type().is_file()
            && is_supported_file(entry.path(), &extensions, &test_patterns, include_tests)
        {
            files.push(entry.path().to_string_lossy().to_string());
        }
    }
    files.sort();
    Ok(files)
}

fn should_include_entry(entry: &walkdir::DirEntry, excludes: &[String]) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let name = entry.file_name().to_string_lossy();
    if entry.file_type().is_dir() {
        return !name.starts_with('.')
            && !excludes
                .iter()
                .any(|e| name.as_ref() == e.trim_end_matches(['/', '\\']));
    }
    // Also exclude individual files matched by name
    !excludes.iter().any(|e| name.as_ref() == e.as_str())
}

// ─── Extracted helper: collect all functions from file reports ─────────────────

fn collect_all_functions(reports: &[FileQualityReport]) -> Vec<&FunctionQualityReport> {
    let mut all: Vec<&FunctionQualityReport> = Vec::new();
    for fr in reports {
        all.extend(fr.file_scope.as_deref());
        all.extend(&fr.functions);
        all.extend(fr.classes.iter().flat_map(|c| &c.methods));
    }
    all
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
