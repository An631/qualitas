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
    let all_fn_flags = collect_all_fn_flags_in_files(report);
    let mut lines = vec![scope_banner("FILE ANALYSIS")];

    lines.extend(render_grade_histogram(
        &format!("{} files", items.len()),
        &items,
    ));
    lines.extend(render_worst_items("Files", &items));
    lines.extend(render_contained_flags("file", &all_fn_flags, items.len()));
    lines
}

fn collect_all_fn_flags_in_files(report: &ProjectQualityReport) -> Vec<&RefactoringFlag> {
    let mut flags = Vec::new();
    for file in &report.files {
        for f in &file.functions {
            flags.extend(&f.flags);
        }
        for cls in &file.classes {
            for m in &cls.methods {
                flags.extend(&m.flags);
            }
        }
    }
    flags
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
    let all_method_flags = collect_all_method_flags(classes);
    let mut lines = vec![scope_banner("CLASS ANALYSIS")];

    lines.extend(render_grade_histogram(
        &format!("{} classes", items.len()),
        &items,
    ));
    lines.extend(render_worst_items("Classes", &items));
    lines.extend(render_contained_flags(
        "class",
        &all_method_flags,
        items.len(),
    ));
    lines
}

fn collect_all_method_flags<'a>(classes: &[&'a ClassQualityReport]) -> Vec<&'a RefactoringFlag> {
    classes
        .iter()
        .flat_map(|c| c.methods.iter().flat_map(|m| &m.flags))
        .collect()
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
    lines.extend(render_flag_type_breakdown(&type_counts));
    lines.push(String::new());
    lines
}

// ─── Pillar Health (function-only) ───────────────────────────────────────────

fn render_pillar_health(fns: &[&FunctionQualityReport]) -> Vec<String> {
    if fns.is_empty() {
        return vec![section_header("Pillar Health"), String::new()];
    }

    let bar_legend = format!(
        "  {:<24} {}  {}  {}",
        "",
        "pass".green(),
        "warn".yellow(),
        "error".red(),
    );

    let mut lines = vec![section_header("Pillar Health (per function)"), bar_legend];

    lines.extend(render_all_pillar_rows(fns));
    lines.push(String::new());
    lines
}

fn render_all_pillar_rows(fns: &[&FunctionQualityReport]) -> Vec<String> {
    let z = |w, e, m, d| ZoneThresholds {
        warn: w,
        error: e,
        max_display: m,
        decimals: d,
    };
    let mut rows = Vec::new();
    rows.extend(pillar_zone_row(
        "Cognitive Flow",
        fns,
        |f| f64::from(f.metrics.cognitive_flow.score),
        &z(13.0, 19.0, 30.0, 0),
    ));
    rows.extend(pillar_zone_row(
        "Data Complexity",
        fns,
        |f| f.metrics.data_complexity.difficulty,
        &z(26.0, 41.0, 60.0, 1),
    ));
    rows.extend(pillar_zone_row(
        "Identifier Refs",
        fns,
        |f| f.metrics.identifier_reference.total_irc,
        &z(41.0, 71.0, 100.0, 1),
    ));
    rows.extend(pillar_zone_row(
        "Dep. Coupling",
        fns,
        |f| f64::from(f.metrics.dependency_coupling.import_count),
        &z(10.0, 15.0, 20.0, 0),
    ));
    rows.extend(pillar_zone_row(
        "Structural LOC",
        fns,
        |f| f64::from(f.metrics.structural.loc),
        &z(41.0, 61.0, 80.0, 0),
    ));
    rows.extend(pillar_zone_row(
        "Structural Params",
        fns,
        |f| f64::from(f.metrics.structural.parameter_count),
        &z(4.0, 5.0, 8.0, 0),
    ));
    rows
}

/// Render a pillar as a colored zone bar with avg/median markers.
///
/// The bar is 30 chars wide. Zone boundaries are proportional to `max_display`.
/// `warn_at` and `error_at` define the zone boundaries.
struct ZoneThresholds {
    warn: f64,
    error: f64,
    max_display: f64,
    decimals: usize,
}

impl ZoneThresholds {
    fn fmt_val(&self, v: f64) -> String {
        match self.decimals {
            0 => format!("{v:.0}"),
            1 => format!("{v:.1}"),
            _ => format!("{v:.2}"),
        }
    }

    fn ranges_display(&self) -> String {
        let f = |v: f64| self.fmt_val(v);
        format!(
            "{} {} {}",
            format!("0\u{2013}{}", f(self.warn - 1.0)).green(),
            format!("> {}\u{2013}{}", f(self.warn), f(self.error)).yellow(),
            format!("> {}+", f(self.error)).red(),
        )
    }
}

fn pillar_zone_row(
    name: &str,
    fns: &[&FunctionQualityReport],
    extract: impl Fn(&FunctionQualityReport) -> f64,
    zone: &ZoneThresholds,
) -> Vec<String> {
    let vals: Vec<f64> = fns.iter().map(|f| extract(f)).collect();
    let avg_val = avg(&vals);
    let med_val = median(&vals);
    let bar = build_zone_bar(avg_val, med_val, zone);
    let avg_label = zone_colorize(&zone.fmt_val(avg_val), avg_val, zone.warn, zone.error);
    let med_label = zone_colorize(&zone.fmt_val(med_val), med_val, zone.warn, zone.error);
    let ranges = zone.ranges_display();
    vec![format!(
        "  {:<24} {}  \u{25b2} avg:{avg_label} \u{25cf} med:{med_label} | Ranges: [{ranges}]",
        name, bar
    )]
}

fn zone_colorize(text: &str, value: f64, warn: f64, error: f64) -> String {
    if value >= error {
        text.red().bold().to_string()
    } else if value >= warn {
        text.yellow().bold().to_string()
    } else {
        text.green().bold().to_string()
    }
}

fn build_zone_bar(avg_val: f64, med_val: f64, zone: &ZoneThresholds) -> String {
    let width = 30usize;
    let mut chars = build_zone_chars(width, zone);
    let to_pos = |v: f64| ((v / zone.max_display).min(1.0) * width as f64).round() as usize;
    place_marker(&mut chars, to_pos(med_val), width, "\u{25cf}");
    place_marker(&mut chars, to_pos(avg_val), width, "\u{25b2}");
    chars.join("")
}

fn build_zone_chars(width: usize, zone: &ZoneThresholds) -> Vec<String> {
    let warn_pos = ((zone.warn / zone.max_display) * width as f64).round() as usize;
    let error_pos = ((zone.error / zone.max_display) * width as f64).round() as usize;
    (0..width)
        .map(|i| zone_char_color(i, warn_pos, error_pos))
        .collect()
}

fn zone_char_color(i: usize, warn_pos: usize, error_pos: usize) -> String {
    if i < warn_pos {
        "\u{2500}".green().to_string()
    } else if i < error_pos {
        "\u{2500}".yellow().to_string()
    } else {
        "\u{2500}".red().to_string()
    }
}

fn place_marker(chars: &mut [String], pos: usize, width: usize, symbol: &str) {
    let idx = pos.min(width - 1);
    // Color the marker based on which zone it falls in
    chars[idx] = symbol.bold().to_string();
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

fn render_contained_flags(
    scope: &str,
    flags: &[&RefactoringFlag],
    item_count: usize,
) -> Vec<String> {
    let (errors, warnings) = count_severities(flags);
    let type_counts = count_flag_types(flags);
    let total = errors + warnings;
    let plural = if scope.ends_with('s') {
        format!("{scope}es")
    } else {
        format!("{scope}s")
    };

    let mut lines = vec![section_header(&format!(
        "Flags (across {item_count} {plural})"
    ))];

    if total == 0 {
        lines.push(format!("  {} No flags", "\u{2713}".green().bold()));
    } else {
        lines.push(format!(
            "  {total} total: {} errors  {} warnings",
            colorize_severity(errors, true),
            colorize_severity(warnings, false),
        ));
        lines.extend(render_flag_type_breakdown(&type_counts));
    }

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

// ─── Flag display helpers ────────────────────────────────────────────────────

fn render_flag_type_breakdown(counts: &HashMap<String, u32>) -> Vec<String> {
    let mut sorted: Vec<(&String, &u32)> = counts.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));
    sorted
        .iter()
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
