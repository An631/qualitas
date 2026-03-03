use std::collections::HashMap;

use colored::Colorize;

use qualitas_core::types::{
    ClassQualityReport, FileQualityReport, FlagType, FunctionQualityReport, Grade,
    ProjectQualityReport, RefactoringFlag, Severity,
};

// ─── Public entry point ──────────────────────────────────────────────────────

pub fn render_executive_summary(report: &ProjectQualityReport) -> String {
    let all_fns = collect_all_fns(report);
    let all_classes = collect_all_classes(report);
    let mut lines = Vec::new();

    lines.extend(render_header(report));
    lines.extend(render_file_section(report));
    if !all_classes.is_empty() {
        lines.extend(render_class_section(&all_classes));
    }
    lines.extend(render_function_section(report, &all_fns));

    lines.join("\n")
}

// ─── Data collection ─────────────────────────────────────────────────────────

fn collect_all_fns(report: &ProjectQualityReport) -> Vec<&FunctionQualityReport> {
    let mut fns = Vec::new();
    for file in &report.files {
        fns.extend(&file.functions);
        for cls in &file.classes {
            fns.extend(&cls.methods);
        }
    }
    fns
}

fn collect_all_classes(report: &ProjectQualityReport) -> Vec<&ClassQualityReport> {
    report.files.iter().flat_map(|f| &f.classes).collect()
}

// ─── Grade helpers ───────────────────────────────────────────────────────────

fn grade_color(grade: Grade, text: &str) -> String {
    match grade {
        Grade::A => text.green().to_string(),
        Grade::B => text.cyan().to_string(),
        Grade::C => text.yellow().to_string(),
        Grade::D => text.red().to_string(),
        Grade::F => text.white().on_red().to_string(),
    }
}

fn score_bar(score: f64) -> String {
    let filled = (score / 10.0).round().min(10.0) as usize;
    let bar = format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(10 - filled)
    );
    grade_color(score_to_grade(score), &bar)
}

fn score_to_grade(score: f64) -> Grade {
    if score >= 80.0 {
        Grade::A
    } else if score >= 65.0 {
        Grade::B
    } else if score >= 50.0 {
        Grade::C
    } else if score >= 35.0 {
        Grade::D
    } else {
        Grade::F
    }
}

fn grade_index(grade: Grade) -> usize {
    match grade {
        Grade::A => 0,
        Grade::B => 1,
        Grade::C => 2,
        Grade::D => 3,
        Grade::F => 4,
    }
}

// ─── Reusable scope item ────────────────────────────────────────────────────

struct ScopeItem {
    name: String,
    score: f64,
    grade: Grade,
    flag_count: usize,
    needs_refactoring: bool,
}

// ─── Header ──────────────────────────────────────────────────────────────────

fn render_header(report: &ProjectQualityReport) -> Vec<String> {
    let title = format!("  qualitas executive summary: {}  ", report.dir_path);
    let width = title.len().max(54);
    let pad_title = format!("{title:<width$}");
    let s = &report.summary;

    vec![
        String::new(),
        format!(
            "{}",
            format!("\u{2554}{}\u{2557}", "\u{2550}".repeat(width))
                .bold()
                .cyan()
        ),
        format!("{}", format!("\u{2551}{pad_title}\u{2551}").bold().cyan()),
        format!(
            "{}",
            format!("\u{255a}{}\u{255d}", "\u{2550}".repeat(width))
                .bold()
                .cyan()
        ),
        String::new(),
        format!(
            "  Score: {} / 100   Grade: {}   {}",
            format!("{:.1}", report.score).bold(),
            grade_color(report.grade, &report.grade.to_string()).bold(),
            score_bar(report.score),
        ),
        format!(
            "  {} files  |  {} functions  |  {} classes",
            s.total_files.to_string().bold(),
            s.total_functions.to_string().bold(),
            s.total_classes.to_string().bold(),
        ),
        String::new(),
    ]
}

// ━━━ FILE SECTION ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn render_file_section(report: &ProjectQualityReport) -> Vec<String> {
    let items = build_file_items(report);
    let flags: Vec<&RefactoringFlag> = report.files.iter().flat_map(|f| &f.flags).collect();
    let mut lines = vec![scope_banner("FILE ANALYSIS")];

    lines.extend(render_grade_histogram(
        &format!("{} files", items.len()),
        &items,
    ));
    lines.extend(render_worst_items("Files", &items));
    lines.extend(render_flag_summary("file", &flags, items.len()));
    lines
}

fn build_file_items(report: &ProjectQualityReport) -> Vec<ScopeItem> {
    report
        .files
        .iter()
        .map(|f| ScopeItem {
            name: short_path(&f.file_path),
            score: f.score,
            grade: f.grade,
            flag_count: count_file_flags(f),
            needs_refactoring: f.needs_refactoring,
        })
        .collect()
}

fn count_file_flags(file: &FileQualityReport) -> usize {
    let mut n = file.flags.len();
    for f in &file.functions {
        n += f.flags.len();
    }
    for cls in &file.classes {
        for m in &cls.methods {
            n += m.flags.len();
        }
    }
    n
}

// ━━━ CLASS SECTION ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn render_class_section(classes: &[&ClassQualityReport]) -> Vec<String> {
    let items = build_class_items(classes);
    let flags: Vec<&RefactoringFlag> = classes.iter().flat_map(|c| &c.flags).collect();
    let mut lines = vec![scope_banner("CLASS ANALYSIS")];

    lines.extend(render_grade_histogram(
        &format!("{} classes", items.len()),
        &items,
    ));
    lines.extend(render_worst_items("Classes", &items));
    lines.extend(render_flag_summary("class", &flags, items.len()));
    lines
}

fn build_class_items(classes: &[&ClassQualityReport]) -> Vec<ScopeItem> {
    classes
        .iter()
        .map(|c| ScopeItem {
            name: c.name.clone(),
            score: c.score,
            grade: c.grade,
            flag_count: c.flags.len() + c.methods.iter().map(|m| m.flags.len()).sum::<usize>(),
            needs_refactoring: c.needs_refactoring,
        })
        .collect()
}

// ━━━ FUNCTION SECTION ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn render_function_section(
    report: &ProjectQualityReport,
    fns: &[&FunctionQualityReport],
) -> Vec<String> {
    let items = build_fn_items(fns);
    let mut lines = vec![scope_banner("FUNCTION ANALYSIS")];

    lines.extend(render_grade_histogram(
        &format!("{} functions", items.len()),
        &items,
    ));
    lines.extend(render_pillar_health(fns));
    lines.extend(render_score_deductions(report, fns));
    lines.extend(render_worst_items("Functions", &items));
    lines.extend(render_function_flags(fns));
    lines
}

fn build_fn_items(fns: &[&FunctionQualityReport]) -> Vec<ScopeItem> {
    fns.iter()
        .map(|f| ScopeItem {
            name: f.name.clone(),
            score: f.score,
            grade: f.grade,
            flag_count: f.flags.len(),
            needs_refactoring: f.needs_refactoring,
        })
        .collect()
}

fn render_function_flags(fns: &[&FunctionQualityReport]) -> Vec<String> {
    let all_flags: Vec<&RefactoringFlag> = fns.iter().flat_map(|f| &f.flags).collect();
    let (errors, warnings) = count_severities(&all_flags);
    let fns_with_flags = fns.iter().filter(|f| !f.flags.is_empty()).count();
    let type_counts = count_flag_types(&all_flags);

    let mut lines = vec![
        section_header(&format!("Risk Flags ({} functions)", fns.len())),
        format!(
            "  {}",
            "Flags fire when individual metrics exceed warning/error thresholds.".dimmed()
        ),
    ];
    lines.extend(format_flag_totals(
        errors,
        warnings,
        fns_with_flags,
        fns.len(),
    ));
    lines.extend(render_top_flag_types(&type_counts));
    lines.push(String::new());
    lines
}

// ─── Pillar Health (function-only) ───────────────────────────────────────────

fn render_pillar_health(fns: &[&FunctionQualityReport]) -> Vec<String> {
    if fns.is_empty() {
        return vec![
            section_header("Pillar Health (per function)"),
            String::new(),
        ];
    }

    let mut lines = vec![
        section_header("Pillar Health (per function)"),
        format!(
            "  {:<26} {:>6}  {:>8}  {}",
            "",
            "avg".dimmed(),
            "median".dimmed(),
            "pass < warn < error".dimmed(),
        ),
    ];

    lines.push(pillar_row(
        "Cognitive Flow",
        fns,
        |f| f64::from(f.metrics.cognitive_flow.score),
        0,
        "0\u{2013}12 < 13\u{2013}19 < 19+",
    ));
    lines.push(pillar_row(
        "Data Complexity",
        fns,
        |f| f.metrics.data_complexity.difficulty,
        1,
        "0\u{2013}25 < 26\u{2013}41 < 41+",
    ));
    lines.push(pillar_row(
        "Identifier References",
        fns,
        |f| f.metrics.identifier_reference.total_irc,
        1,
        "0\u{2013}40 < 41\u{2013}71 < 71+",
    ));
    lines.push(pillar_row(
        "Dependency Coupling",
        fns,
        |f| f64::from(f.metrics.dependency_coupling.import_count),
        0,
        "0\u{2013}9 < 10\u{2013}15 < 15+",
    ));
    lines.push(pillar_row(
        "Structural LOC",
        fns,
        |f| f64::from(f.metrics.structural.loc),
        0,
        "0\u{2013}40 < 41\u{2013}61 < 61+",
    ));
    lines.push(pillar_row(
        "Structural Params",
        fns,
        |f| f64::from(f.metrics.structural.parameter_count),
        0,
        "0\u{2013}3 < 4\u{2013}5 < 5+",
    ));

    lines.push(String::new());
    lines
}

fn pillar_row(
    name: &str,
    fns: &[&FunctionQualityReport],
    extract: impl Fn(&FunctionQualityReport) -> f64,
    decimals: usize,
    thresholds: &str,
) -> String {
    let vals: Vec<f64> = fns.iter().map(|f| extract(f)).collect();
    let fmt = |v: f64| match decimals {
        0 => format!("{v:.0}"),
        1 => format!("{v:.1}"),
        _ => format!("{v:.2}"),
    };
    format!(
        "  {:<26} {:>6}  {:>8}  {}",
        name,
        fmt(avg(&vals)).bold(),
        fmt(median(&vals)).bold(),
        thresholds.dimmed(),
    )
}

// ─── Score Deductions (function-only) ────────────────────────────────────────

fn render_score_deductions(
    report: &ProjectQualityReport,
    fns: &[&FunctionQualityReport],
) -> Vec<String> {
    if fns.is_empty() {
        return vec![section_header("Score Deductions"), String::new()];
    }

    let pillars = [
        (
            "Cognitive Flow",
            loc_weighted(fns, |f| f.score_breakdown.cfc_penalty),
            30.0,
        ),
        (
            "Data Complexity",
            loc_weighted(fns, |f| f.score_breakdown.dci_penalty),
            25.0,
        ),
        (
            "Identifier References",
            loc_weighted(fns, |f| f.score_breakdown.irc_penalty),
            20.0,
        ),
        (
            "Dependency Coupling",
            loc_weighted(fns, |f| f.score_breakdown.dc_penalty),
            15.0,
        ),
        (
            "Structural",
            loc_weighted(fns, |f| f.score_breakdown.sm_penalty),
            10.0,
        ),
    ];

    let total_deducted = 100.0 - report.score;

    let mut lines = vec![
        section_header("Score Deductions (points lost from 100)"),
        format!(
            "  {}",
            "Each pillar deducts up to its weight. Weighted by lines of code.".dimmed()
        ),
    ];

    for (name, penalty, max_w) in &pillars {
        let fill = (penalty / max_w * 10.0).round().min(10.0) as usize;
        let bar = format!(
            "{}{}",
            "\u{2588}".repeat(fill),
            "\u{2591}".repeat(10 - fill)
        );
        let cfn = penalty_color(*penalty, *max_w);
        lines.push(format!(
            "  {:<24} {:<13} {:>4.1} / {:.0} pts",
            name,
            cfn(&bar),
            penalty,
            max_w
        ));
    }

    lines.push(format!(
        "  {:<24} {:<13} {:>4.1} / 100 pts",
        "Total deducted".bold(),
        "",
        total_deducted
    ));
    lines.push(String::new());
    lines
}

fn penalty_color(penalty: f64, max: f64) -> impl Fn(&str) -> String {
    let ratio = penalty / max;
    move |text: &str| {
        if ratio < 0.25 {
            text.green().to_string()
        } else if ratio < 0.5 {
            text.yellow().to_string()
        } else {
            text.red().to_string()
        }
    }
}

fn loc_weighted(
    fns: &[&FunctionQualityReport],
    extract: impl Fn(&FunctionQualityReport) -> f64,
) -> f64 {
    let total_loc: f64 = fns
        .iter()
        .map(|f| f64::from(f.metrics.structural.loc.max(1)))
        .sum();
    if total_loc == 0.0 {
        return 0.0;
    }
    let weighted: f64 = fns
        .iter()
        .map(|f| extract(f) * f64::from(f.metrics.structural.loc.max(1)))
        .sum();
    weighted / total_loc
}

// ─── Generic: Grade Histogram ────────────────────────────────────────────────

fn render_grade_histogram(label: &str, items: &[ScopeItem]) -> Vec<String> {
    let counts = count_grades(items);
    let max_c = *counts.iter().max().unwrap_or(&1);
    let max_c = max_c.max(1);
    let total = items.len() as u32;

    let grades = [
        (Grade::A, "A"),
        (Grade::B, "B"),
        (Grade::C, "C"),
        (Grade::D, "D"),
        (Grade::F, "F"),
    ];
    let mut lines = vec![section_header(&format!("Grade Distribution ({label})"))];
    for (i, (grade, lbl)) in grades.iter().enumerate() {
        lines.push(format_histogram_row(*grade, lbl, counts[i], max_c, total));
    }
    lines.push(String::new());
    lines
}

fn count_grades(items: &[ScopeItem]) -> [u32; 5] {
    let mut counts = [0u32; 5];
    for item in items {
        counts[grade_index(item.grade)] += 1;
    }
    counts
}

fn format_histogram_row(
    grade: Grade,
    label: &str,
    count: u32,
    max_count: u32,
    total: u32,
) -> String {
    let bar_width = 50;
    let filled = (f64::from(count) / f64::from(max_count) * bar_width as f64).round() as usize;
    let bar = format!(
        "{}{}",
        grade_color(grade, &"\u{2588}".repeat(filled)),
        "\u{2591}".repeat(bar_width - filled),
    );
    format!("  {label} {bar}  {count:>4} ({})", pct(count, total))
}

// ─── Generic: Worst Items ────────────────────────────────────────────────────

fn render_worst_items(scope: &str, items: &[ScopeItem]) -> Vec<String> {
    let mut sorted: Vec<&ScopeItem> = items.iter().collect();
    sorted.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines = vec![section_header(&format!("{scope} by Score (worst first)"))];
    let show = sorted.len().min(10);
    for item in sorted.iter().take(show) {
        lines.push(format_scope_item_line(item));
    }
    if sorted.len() > show {
        lines.push(format!(
            "  {} ... and {} more",
            "\u{2026}".dimmed(),
            sorted.len() - show
        ));
    }
    lines.push(String::new());
    lines
}

fn format_scope_item_line(item: &ScopeItem) -> String {
    let icon = if item.needs_refactoring {
        "\u{2717}".red()
    } else {
        "\u{2713}".green()
    };
    let suffix = if item.flag_count > 0 {
        format!("({} flags)", item.flag_count).dimmed().to_string()
    } else {
        String::new()
    };
    format!(
        "  {icon} {:<35} {:>5.1}  {}  {suffix}",
        item.name,
        item.score,
        grade_color(item.grade, &item.grade.to_string()),
    )
}

// ─── Generic: Flag Summary ───────────────────────────────────────────────────

fn render_flag_summary(scope: &str, flags: &[&RefactoringFlag], total_items: usize) -> Vec<String> {
    let (errors, warnings) = count_severities(flags);
    let plural = if scope.ends_with('s') {
        format!("{scope}es")
    } else {
        format!("{scope}s")
    };
    let mut lines = vec![section_header(&format!("Flags ({total_items} {plural})"))];
    lines.extend(format_scope_flag_totals(scope, errors, warnings));
    lines.push(String::new());
    lines
}

// ─── Shared flag counting ─────────────────────────────────────────────────

fn count_severities(flags: &[&RefactoringFlag]) -> (u32, u32) {
    let errors = flags
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count() as u32;
    let warnings = flags
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .count() as u32;
    (errors, warnings)
}

fn count_flag_types(flags: &[&RefactoringFlag]) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for flag in flags {
        *counts
            .entry(flag_type_display(&flag.flag_type))
            .or_insert(0) += 1;
    }
    counts
}

fn format_flag_totals(errors: u32, warnings: u32, with_flags: usize, total: usize) -> Vec<String> {
    let sum = errors + warnings;
    if sum == 0 {
        return vec![format!("  {} No flags", "\u{2713}".green().bold())];
    }
    vec![format!(
        "  {sum} total: {} errors  {} warnings  in {with_flags} of {total} functions",
        colorize_severity(errors, true),
        colorize_severity(warnings, false),
    )]
}

fn format_scope_flag_totals(scope: &str, errors: u32, warnings: u32) -> Vec<String> {
    let sum = errors + warnings;
    if sum == 0 {
        return vec![format!(
            "  {} No {scope}-level flags",
            "\u{2713}".green().bold()
        )];
    }
    vec![format!(
        "  {sum} total: {} errors  {} warnings",
        colorize_severity(errors, true),
        colorize_severity(warnings, false),
    )]
}

// ─── Flag display helpers ────────────────────────────────────────────────────

fn render_top_flag_types(counts: &HashMap<String, u32>) -> Vec<String> {
    let mut sorted: Vec<(&String, &u32)> = counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    sorted
        .iter()
        .take(3)
        .map(|(name, count)| format!("    \u{2022} {name} ({count}\u{00d7})"))
        .collect()
}

fn colorize_severity(n: u32, is_error: bool) -> String {
    if n == 0 {
        "0".green().to_string()
    } else if is_error {
        n.to_string().red().bold().to_string()
    } else {
        n.to_string().yellow().bold().to_string()
    }
}

fn flag_type_display(ft: &FlagType) -> String {
    flag_type_metric_name(ft).unwrap_or_else(|| flag_type_structural_name(ft))
}

fn flag_type_metric_name(ft: &FlagType) -> Option<String> {
    match ft {
        FlagType::HighCognitiveFlow => Some("Cognitive flow complexity".to_string()),
        FlagType::HighDataComplexity => Some("Data complexity".to_string()),
        FlagType::HighIdentifierChurn => Some("Identifier reference churn".to_string()),
        FlagType::HighCoupling => Some("High coupling".to_string()),
        FlagType::HighHalsteadEffort => Some("High Halstead effort".to_string()),
        _ => None,
    }
}

fn flag_type_structural_name(ft: &FlagType) -> String {
    match ft {
        FlagType::TooManyParams => "Too many parameters",
        FlagType::TooLong => "Function too long",
        FlagType::DeepNesting => "Deep nesting",
        FlagType::ExcessiveReturns => "Excessive returns",
        _ => "Unknown flag",
    }
    .to_string()
}

// ─── Formatting utilities ────────────────────────────────────────────────────

fn scope_banner(title: &str) -> String {
    let dashes = "\u{2501}".repeat(55 - title.len().min(50));
    format!("{} {title} {dashes}", "\u{2501}\u{2501}\u{2501}")
        .bold()
        .to_string()
}

fn section_header(title: &str) -> String {
    let dashes = "\u{2500}".repeat(55 - title.len().min(50));
    format!("{} {title} {dashes}", "\u{2500}\u{2500}\u{2500}".dimmed())
}

fn short_path(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn pct(n: u32, total: u32) -> String {
    if total == 0 {
        "0.0%".to_string()
    } else {
        format!("{:.1}%", f64::from(n) / f64::from(total) * 100.0)
    }
}

fn avg(vals: &[f64]) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    vals.iter().sum::<f64>() / vals.len() as f64
}

fn median(vals: &[f64]) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    let mut sorted = vals.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        f64::midpoint(sorted[mid - 1], sorted[mid])
    } else {
        sorted[mid]
    }
}
