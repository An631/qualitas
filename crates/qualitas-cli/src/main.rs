mod config;
mod reporters;

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use clap::Parser;
use rayon::prelude::*;
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

    /// Output file path for report formats (json, markdown). Defaults to qualitas-report.<ext>
    #[arg(short = 'o', long)]
    output: Option<String>,
}

// ─── Default file-collection settings ─────────────────────────────────────────

/// Only .git is universally excluded — all other excludes come from qualitas.config.js.
const DEFAULT_EXCLUDE: &[&str] = &[".git"];

/// Skip files larger than 1 MB — these are almost certainly bundled/generated.
const MAX_FILE_SIZE: u64 = 1_024 * 1_024;

// ─── Output mode: console vs report file ──────────────────────────────────────

enum OutputMode {
    /// Print to stdout. Exit code reflects threshold/flag violations.
    Console,
    /// Write to file. Exit code is always 0.
    Report(String),
}

fn resolve_output_mode(format: &str, cli_output: Option<&str>, input_path: &str) -> OutputMode {
    let default_name = match format {
        "json" => Some("qualitas-report.json"),
        "markdown" => Some("qualitas-report.md"),
        _ => None,
    };
    match default_name {
        Some(name) => {
            let path = cli_output.unwrap_or(name);
            OutputMode::Report(resolve_report_path(path, input_path))
        }
        None => OutputMode::Console,
    }
}

fn resolve_report_path(path: &str, input_path: &str) -> String {
    if Path::new(path).is_absolute() {
        return path.to_string();
    }
    let base = if Path::new(input_path).is_dir() {
        input_path
    } else {
        Path::new(input_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".")
    };
    Path::new(base).join(path).to_string_lossy().to_string()
}

// ─── Analysis result ─────────────────────────────────────────────────────────

enum AnalysisResult {
    File(FileQualityReport),
    Project(ProjectQualityReport),
}

// ─── Phase 1: Analyze ────────────────────────────────────────────────────────

fn analyze(
    path: &str,
    options: &AnalysisOptions,
    config: &QualitasConfig,
) -> Result<AnalysisResult, String> {
    if Path::new(path).is_dir() {
        analyze_directory(path, options, config)
    } else {
        let mut opts = options.clone();
        opts.flag_overrides = resolve_flag_overrides(path, config);
        let report = analyze_file(path, &opts)?;
        Ok(AnalysisResult::File(report))
    }
}

fn analyze_directory(
    path: &str,
    options: &AnalysisOptions,
    config: &QualitasConfig,
) -> Result<AnalysisResult, String> {
    let include_tests = options.include_tests.unwrap_or(false);
    let threshold = options.refactoring_threshold.unwrap_or(65.0);
    let files = collect_files(path, include_tests, config)?;

    if files.is_empty() {
        return Err(format!("no supported files found in {path}"));
    }

    let file_reports = analyze_all_files(&files, options, config);
    let report = build_project_report(path, file_reports, threshold);
    Ok(AnalysisResult::Project(report))
}

// ─── Phase 2: Output ─────────────────────────────────────────────────────────

fn render(result: &AnalysisResult, format: &str) -> String {
    match result {
        AnalysisResult::File(r) => format_file_output(r, format),
        AnalysisResult::Project(r) => format_project_output(r, format),
    }
}

fn output(rendered: &str, mode: &OutputMode) -> Result<(), String> {
    match mode {
        OutputMode::Console => write_stdout(rendered),
        OutputMode::Report(path) => {
            std::fs::write(path, rendered).map_err(|e| format!("cannot write {path}: {e}"))?;
            eprintln!("qualitas: report written to {path}");
        }
    }
    Ok(())
}

// ─── Phase 3: Exit code ──────────────────────────────────────────────────────

fn compute_exit_code(
    result: &AnalysisResult,
    threshold: f64,
    fail_on_flags: Option<&str>,
    mode: &OutputMode,
) -> i32 {
    if matches!(mode, OutputMode::Report(_)) {
        return 0;
    }
    let below = match result {
        AnalysisResult::File(r) => check_file_threshold(r, threshold, fail_on_flags),
        AnalysisResult::Project(r) => check_project_threshold(r, threshold, fail_on_flags),
    };
    i32::from(below)
}

fn check_file_threshold(
    report: &FileQualityReport,
    threshold: f64,
    fail_on_flags: Option<&str>,
) -> bool {
    report.score < threshold
        || report.functions.iter().any(|f| f.score < threshold)
        || report
            .file_scope
            .as_ref()
            .is_some_and(|fs| fs.score < threshold)
        || has_flags_at_severity(
            &report.functions,
            report.file_scope.as_deref(),
            fail_on_flags,
        )
}

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

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    let config = config::load_config(&cli.path);
    validate_path(&cli.path);

    match run(&cli, &config) {
        Ok(code) => process::exit(code),
        Err(e) => {
            eprintln!("qualitas error: {e}");
            process::exit(2);
        }
    }
}

fn run(cli: &Cli, config: &QualitasConfig) -> Result<i32, String> {
    let (options, format) = config::merge_config(cli, config);
    let mode = resolve_output_mode(&format, cli.output.as_deref(), &cli.path);

    let result = analyze(&cli.path, &options, config)?;

    output(&render(&result, &format), &mode)?;

    let fail_on_flags = cli
        .fail_on_flags
        .as_deref()
        .or(config.fail_on_flags.as_deref());
    let threshold = options.refactoring_threshold.unwrap_or(65.0);
    Ok(compute_exit_code(&result, threshold, fail_on_flags, &mode))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn validate_path(path: &str) {
    if !Path::new(path).exists() {
        eprintln!("qualitas: path not found: {path}");
        process::exit(2);
    }
}

fn write_stdout(text: &str) {
    let stdout = std::io::stdout();
    let mut writer = std::io::BufWriter::new(stdout.lock());
    let _ = writer.write_all(text.as_bytes());
    let _ = writer.write_all(b"\n");
    let _ = writer.flush();
}

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

// ─── File analysis ──────────────────────────────────────────────────────────

// ─── Folder timing ──────────────────────────────────────────────────────────

struct FolderTiming {
    file_count: u32,
    total_ms: u64,
}

const SLOW_FOLDER_THRESHOLD_MS: u64 = 30_000;
const PERF_SUMMARY_MIN_TOTAL_MS: u64 = 5_000;

fn parent_dir_key(file_path: &str) -> String {
    Path::new(file_path)
        .parent()
        .map_or_else(|| ".".to_string(), |p| p.to_string_lossy().to_string())
}

fn record_timing(timings: &Arc<Mutex<HashMap<String, FolderTiming>>>, dir: &str, elapsed_ms: u64) {
    let mut map = timings
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = map.entry(dir.to_string()).or_insert(FolderTiming {
        file_count: 0,
        total_ms: 0,
    });
    entry.file_count += 1;
    entry.total_ms += elapsed_ms;

    if entry.total_ms >= SLOW_FOLDER_THRESHOLD_MS
        && entry.total_ms - elapsed_ms < SLOW_FOLDER_THRESHOLD_MS
    {
        eprintln!(
            "qualitas: slow folder \u{2014} {} ({} files, {:.1}s)",
            dir,
            entry.file_count,
            entry.total_ms as f64 / 1000.0,
        );
        eprintln!("  \u{2192} Consider adding '{dir}' to the exclude list in qualitas.config.js");
    }
}

fn print_perf_summary(timings: &HashMap<String, FolderTiming>) {
    let total_ms: u64 = timings.values().map(|t| t.total_ms).sum();
    if total_ms < PERF_SUMMARY_MIN_TOTAL_MS {
        return;
    }

    let mut sorted: Vec<_> = timings.iter().collect();
    sorted.sort_by(|a, b| b.1.total_ms.cmp(&a.1.total_ms));

    eprintln!("\nqualitas: performance summary (slowest folders):");
    for (dir, timing) in sorted.iter().take(5) {
        eprintln!(
            "  {:<50} {:>5} files  {:>6.1}s",
            dir,
            timing.file_count,
            timing.total_ms as f64 / 1000.0,
        );
    }
}

// ─── File analysis ──────────────────────────────────────────────────────────

fn analyze_all_files(
    files: &[String],
    options: &AnalysisOptions,
    config: &QualitasConfig,
) -> Vec<FileQualityReport> {
    let timings: Arc<Mutex<HashMap<String, FolderTiming>>> = Arc::new(Mutex::new(HashMap::new()));

    let results: Vec<_> = files
        .par_iter()
        .map(|file_path| {
            let start = Instant::now();
            let mut opts = options.clone();
            opts.flag_overrides = resolve_flag_overrides(file_path, config);
            let result = match analyze_file(file_path, &opts) {
                Ok(report) => Some(report),
                Err(e) => {
                    eprintln!("qualitas: skipping {file_path}: {e}");
                    None
                }
            };
            let dir = parent_dir_key(file_path);
            record_timing(&timings, &dir, start.elapsed().as_millis() as u64);
            result
        })
        .collect();

    let map = timings
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    print_perf_summary(&map);

    results.into_iter().flatten().collect()
}

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

// ─── Threshold / flag helpers ───────────────────────────────────────────────

fn fn_has_flags(f: &FunctionQualityReport, level: &str) -> bool {
    match level {
        "warn" => !f.flags.is_empty(),
        "error" => f
            .flags
            .iter()
            .any(|flag| flag.severity == qualitas_core::types::Severity::Error),
        _ => false,
    }
}

fn has_flags_at_severity(
    functions: &[FunctionQualityReport],
    file_scope: Option<&FunctionQualityReport>,
    fail_on_flags: Option<&str>,
) -> bool {
    let Some(level) = fail_on_flags else {
        return false;
    };
    functions.iter().any(|f| fn_has_flags(f, level))
        || file_scope.is_some_and(|fs| fn_has_flags(fs, level))
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
        .max_open(50)
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
        return !name.starts_with('.') && !is_excluded_dir(&name, excludes);
    }
    if entry.file_type().is_file() {
        if let Ok(meta) = entry.metadata() {
            if meta.len() > MAX_FILE_SIZE {
                return false;
            }
        }
    }
    !excludes.iter().any(|e| name.as_ref() == e.as_str())
}

fn is_excluded_dir(name: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|e| {
        let pattern = e.trim_end_matches(['/', '\\']);
        name == pattern || name.starts_with(&format!("{pattern}-"))
    })
}

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

// ─── Adapter extensions and test patterns ──────────────────────────────────

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
        "py" | "pyi" => Some("python".to_string()),
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

// ─── Project report builder ───────────────────────────────────────────────────

fn collect_all_functions(reports: &[FileQualityReport]) -> Vec<&FunctionQualityReport> {
    let mut all: Vec<&FunctionQualityReport> = Vec::new();
    for fr in reports {
        all.extend(fr.file_scope.as_deref());
        all.extend(&fr.functions);
        all.extend(fr.classes.iter().flat_map(|c| &c.methods));
    }
    all
}

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

fn find_worst_functions(functions: &[&FunctionQualityReport]) -> Vec<FunctionQualityReport> {
    let mut worst: Vec<FunctionQualityReport> = functions.iter().map(|f| (*f).clone()).collect();
    worst.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    worst
}

fn compute_weighted_score(all_fns: &[&FunctionQualityReport]) -> f64 {
    let scores: Vec<(f64, u32)> = all_fns
        .iter()
        .map(|f| (f.score, f.metrics.structural.loc.max(1)))
        .collect();
    if scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&scores)
    }
}

fn build_summary(
    file_reports: &[FileQualityReport],
    all_fns: &[&FunctionQualityReport],
    score: f64,
) -> ProjectSummary {
    ProjectSummary {
        total_files: file_reports.len() as u32,
        total_functions: all_fns.len() as u32,
        total_classes: file_reports.iter().map(|f| f.class_count).sum(),
        flagged_files: file_reports.iter().filter(|f| f.needs_refactoring).count() as u32,
        flagged_functions: all_fns.iter().filter(|f| f.needs_refactoring).count() as u32,
        average_score: score,
        grade_distribution: build_grade_distribution(all_fns),
    }
}

fn build_project_report(
    dir_path: &str,
    file_reports: Vec<FileQualityReport>,
    threshold: f64,
) -> ProjectQualityReport {
    let all_fns = collect_all_functions(&file_reports);
    let score = compute_weighted_score(&all_fns);

    ProjectQualityReport {
        dir_path: dir_path.to_string(),
        score,
        grade: grade_from_score(score, None),
        needs_refactoring: score < threshold,
        summary: build_summary(&file_reports, &all_fns, score),
        worst_functions: find_worst_functions(&all_fns),
        files: file_reports,
    }
}
