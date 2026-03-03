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
    let mut errors = 0u32;
    let mut warnings = 0u32;
    let mut fns_with_flags = 0u32;
    let mut type_counts: HashMap<String, u32> = HashMap::new();

    for f in fns {
        if !f.flags.is_empty() {
            fns_with_flags += 1;
        }
        for flag in &f.flags {
            match flag.severity {
                Severity::Error => errors += 1,
                Severity::Warning => warnings += 1,
                Severity::Info => {}
            }
            *type_counts
                .entry(flag_type_display(&flag.flag_type))
                .or_insert(0) += 1;
        }
    }

    let total = errors + warnings;
    let mut lines = vec![section_header(&format!(
        "Risk Flags ({} functions)",
        fns.len()
    ))];
    lines.push(format!(
        "  {}",
        "Flags fire when individual metrics exceed warning/error thresholds.".dimmed()
    ));

    if total == 0 {
        lines.push(format!("  {} No flags", "\u{2713}".green().bold()));
    } else {
        lines.push(format!(
            "  {total} total: {} errors  {} warnings  in {fns_with_flags} of {} functions",
            colorize_severity(errors, true),
            colorize_severity(warnings, false),
            fns.len(),
        ));
        lines.extend(render_top_flag_types(&type_counts));
    }

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
            "  {:<30} {:>7}  {:>9}  {}",
            "",
            "avg".dimmed(),
            "median".dimmed(),
            "expected range".dimmed(),
        ),
    ];

    lines.push(pillar_row(
        "Cognitive Flow",
        fns,
        |f| f64::from(f.metrics.cognitive_flow.score),
        0,
        "<13 good, 13\u{2013}19 warn, >19 high",
    ));
    lines.push(pillar_row(
        "Data Complexity",
        fns,
        |f| f.metrics.data_complexity.difficulty,
        1,
        "<26 good, 26\u{2013}41 warn, >41 high",
    ));
    lines.push(pillar_row(
        "Identifier References",
        fns,
        |f| f.metrics.identifier_reference.total_irc,
        1,
        "<41 good, 41\u{2013}71 warn, >71 high",
    ));
    lines.push(pillar_row(
        "Dependency Coupling",
        fns,
        |f| f.metrics.dependency_coupling.raw_score,
        2,
        "<0.3 good, 0.3\u{2013}0.6 warn, >0.6 high",
    ));
    lines.push(structural_row(fns));

    lines.push(String::new());
    lines
}

fn pillar_row(
    name: &str,
    fns: &[&FunctionQualityReport],
    extract: impl Fn(&FunctionQualityReport) -> f64,
    decimals: usize,
    hint: &str,
) -> String {
    let vals: Vec<f64> = fns.iter().map(|f| extract(f)).collect();
    let fmt = |v: f64| match decimals {
        0 => format!("{v:.0}"),
        1 => format!("{v:.1}"),
        _ => format!("{v:.2}"),
    };
    format!(
        "  {:<30} {:>7}  {:>9}  {}",
        name,
        fmt(avg(&vals)).bold(),
        fmt(median(&vals)).bold(),
        hint.dimmed(),
    )
}

fn structural_row(fns: &[&FunctionQualityReport]) -> String {
    let locs: Vec<f64> = fns
        .iter()
        .map(|f| f64::from(f.metrics.structural.loc))
        .collect();
    let params: Vec<f64> = fns
        .iter()
        .map(|f| f64::from(f.metrics.structural.parameter_count))
        .collect();
    format!(
        "  {:<30} {} LOC, {} params      {}",
        "Structural",
        format!("{:.0}", avg(&locs)).bold(),
        format!("{:.1}", avg(&params)).bold(),
        "<41 LOC good, <4 params good".dimmed(),
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
    let mut counts = [0u32; 5];
    for item in items {
        counts[grade_index(item.grade)] += 1;
    }
    let grades = [
        (Grade::A, "A", counts[0]),
        (Grade::B, "B", counts[1]),
        (Grade::C, "C", counts[2]),
        (Grade::D, "D", counts[3]),
        (Grade::F, "F", counts[4]),
    ];
    let max_c = grades.iter().map(|(_, _, c)| *c).max().unwrap_or(1).max(1);
    let total = items.len() as u32;

    let bar_width = 50;
    let mut lines = vec![section_header(&format!("Grade Distribution ({label})"))];
    for (grade, lbl, count) in &grades {
        let filled = if max_c > 0 {
            (f64::from(*count) / f64::from(max_c) * bar_width as f64).round() as usize
        } else {
            0
        };
        let bar = format!(
            "{}{}",
            grade_color(*grade, &"\u{2588}".repeat(filled)),
            "\u{2591}".repeat(bar_width - filled),
        );
        let p = pct(*count, total);
        lines.push(format!("  {lbl} {bar}  {count:>4} ({p})"));
    }
    lines.push(String::new());
    lines
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
        lines.push(format!(
            "  {icon} {:<35} {:>5.1}  {}  {suffix}",
            item.name,
            item.score,
            grade_color(item.grade, &item.grade.to_string()),
        ));
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

// ─── Generic: Flag Summary ───────────────────────────────────────────────────

fn render_flag_summary(scope: &str, flags: &[&RefactoringFlag], total_items: usize) -> Vec<String> {
    let mut errors = 0u32;
    let mut warnings = 0u32;
    for flag in flags {
        match flag.severity {
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
            Severity::Info => {}
        }
    }
    let total = errors + warnings;
    let plural = if scope.ends_with('s') {
        format!("{scope}es")
    } else {
        format!("{scope}s")
    };
    let mut lines = vec![section_header(&format!("Flags ({total_items} {plural})"))];
    if total == 0 {
        lines.push(format!(
            "  {} No {scope}-level flags",
            "\u{2713}".green().bold()
        ));
    } else {
        lines.push(format!(
            "  {total} total: {} errors  {} warnings",
            colorize_severity(errors, true),
            colorize_severity(warnings, false),
        ));
    }
    lines.push(String::new());
    lines
}

// ─── Flag helpers ────────────────────────────────────────────────────────────

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
    match ft {
        FlagType::HighCognitiveFlow => "Cognitive flow complexity".to_string(),
        FlagType::HighDataComplexity => "Data complexity".to_string(),
        FlagType::HighIdentifierChurn => "Identifier reference churn".to_string(),
        FlagType::TooManyParams => "Too many parameters".to_string(),
        FlagType::TooLong => "Function too long".to_string(),
        FlagType::DeepNesting => "Deep nesting".to_string(),
        FlagType::HighCoupling => "High coupling".to_string(),
        FlagType::ExcessiveReturns => "Excessive returns".to_string(),
        FlagType::HighHalsteadEffort => "High Halstead effort".to_string(),
    }
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
