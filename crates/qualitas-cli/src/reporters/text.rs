use colored::Colorize;

use qualitas_core::scorer::thresholds::grade_from_score;
use qualitas_core::types::{
    ClassQualityReport, FileQualityReport, FunctionQualityReport, Grade, ProjectQualityReport,
    RefactoringFlag, Severity,
};

pub struct TextReporterOptions {
    pub verbose: bool,
    pub flagged_only: bool,
    pub scope: String,
}

impl Default for TextReporterOptions {
    fn default() -> Self {
        Self {
            verbose: false,
            flagged_only: false,
            scope: "function".to_string(),
        }
    }
}

// ─── Grade colors ─────────────────────────────────────────────────────────────

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
    let filled = (score / 10.0).round() as usize;
    let filled = filled.min(10);
    let bar = format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(10 - filled)
    );
    grade_color(grade_from_score(score, None), &bar)
}

// ─── Flag rendering ───────────────────────────────────────────────────────────

fn render_flag(flag: &RefactoringFlag, indent: &str) -> String {
    let sev = match flag.severity {
        Severity::Error => "[error]".red().to_string(),
        Severity::Warning => "[warn] ".yellow().to_string(),
        Severity::Info => "[info] ".dimmed().to_string(),
    };
    let arrow = "\u{2192}".dimmed().to_string();
    format!(
        "{indent}{sev} {}\n{indent}       {arrow} {}",
        flag.message, flag.suggestion
    )
}

// ─── Function row ─────────────────────────────────────────────────────────────

fn render_function(func: &FunctionQualityReport, opts: &TextReporterOptions) -> String {
    let icon = if func.needs_refactoring {
        "\u{2717}".red().to_string()
    } else {
        "\u{2713}".green().to_string()
    };
    let grade = grade_color(func.grade, &format!("[{}]", func.grade));
    let score_str = if func.needs_refactoring {
        format!("score: {}", func.score as u32).red().to_string()
    } else {
        format!("score: {}", func.score as u32).green().to_string()
    };

    let mut line = format!("  {icon} {}()  {grade}  {score_str}", func.name.bold());

    if func.needs_refactoring {
        line += &"  \u{2190} needs refactoring".dimmed().to_string();
    }

    let mut lines = vec![line];

    if opts.verbose {
        let m = &func.metrics;
        lines.push(format!(
            "      CFC: {} ({})  DCI: {:.1} ({})  IRC: {:.1}  Params: {}  LOC: {}",
            m.cognitive_flow.score,
            grade_from_score(100.0 - f64::from(m.cognitive_flow.score) * 4.0, None),
            m.data_complexity.difficulty,
            grade_from_score(100.0 - m.data_complexity.difficulty, None),
            m.identifier_reference.total_irc,
            m.structural.parameter_count,
            m.structural.loc,
        ));
    }

    if !func.flags.is_empty() {
        lines.push(format!("    {}", "Flags:".dimmed()));
        for flag in &func.flags {
            lines.push(render_flag(flag, "    "));
        }
    }

    lines.join("\n")
}

// ─── File report ──────────────────────────────────────────────────────────────

fn render_file_summary_line(report: &FileQualityReport) -> String {
    let flag_count = report.flagged_function_count;
    let total = report.function_count;
    let score_text = format!("{} \u{2014} {:.1}", report.grade, report.score);
    let grade_str = grade_color(report.grade, &score_text);

    let status = if flag_count > 0 {
        format!("  \u{2014} {flag_count} of {total} function(s) need refactoring")
            .red()
            .to_string()
    } else {
        format!("  \u{2014} all {total} function(s) within thresholds")
            .green()
            .to_string()
    };

    format!("File: {grade_str}{status}")
}

// ─── Extracted helper: render file header lines ───────────────────────────────

fn render_file_header(report: &FileQualityReport) -> Vec<String> {
    let grade_str = grade_color(report.grade, &report.grade.to_string());

    vec![
        String::new(),
        format!("qualitas: {}", report.file_path).bold().to_string(),
        format!(
            "{}  score: {:.1}  grade: {grade_str}",
            score_bar(report.score),
            report.score,
        ),
        String::new(),
    ]
}

// ─── Extracted helper: render flags for a class ───────────────────────────────

fn render_class_flags(flags: &[RefactoringFlag]) -> Vec<String> {
    let mut lines = Vec::new();
    if !flags.is_empty() {
        lines.push(format!("    {}", "Flags:".dimmed()));
        for flag in flags {
            lines.push(render_flag(flag, "    "));
        }
    }
    lines
}

// ─── Extracted helper: render class scope ─────────────────────────────────────

fn render_class_scope(report: &FileQualityReport, opts: &TextReporterOptions) -> Vec<String> {
    let mut lines = Vec::new();
    for cls in &report.classes {
        if opts.flagged_only && cls.methods.iter().all(|m| m.flags.is_empty()) {
            continue;
        }
        lines.push(format!(
            "  {}  {}  score: {:.1}",
            format!("class {}", cls.name).cyan(),
            grade_color(cls.grade, &cls.grade.to_string()),
            cls.score,
        ));
        lines.extend(render_class_flags(&cls.flags));
    }
    lines
}

// ─── Extracted helper: filter functions by flagged_only setting ────────────────

fn filter_functions(
    functions: &[FunctionQualityReport],
    flagged_only: bool,
) -> Vec<&FunctionQualityReport> {
    if flagged_only {
        functions.iter().filter(|f| !f.flags.is_empty()).collect()
    } else {
        functions.iter().collect()
    }
}

// ─── Extracted helper: render class with its methods in function scope ─────────

fn render_class_methods(cls: &ClassQualityReport, opts: &TextReporterOptions) -> Vec<String> {
    let class_fns = filter_functions(&cls.methods, opts.flagged_only);
    if opts.flagged_only && class_fns.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![
        String::new(),
        format!(
            "  {}  {}  score: {:.1}",
            format!("class {}", cls.name).cyan(),
            grade_color(cls.grade, &cls.grade.to_string()),
            cls.score,
        ),
    ];
    for m in &class_fns {
        lines.push(render_function(m, opts));
    }
    lines
}

// ─── Extracted helper: render function scope ──────────────────────────────────

fn render_function_scope(report: &FileQualityReport, opts: &TextReporterOptions) -> Vec<String> {
    let mut lines = Vec::new();

    // Render file-scope before functions
    if let Some(fs) = &report.file_scope {
        if !opts.flagged_only || !fs.flags.is_empty() {
            lines.push(render_function(fs, opts));
        }
    }

    let fns = filter_functions(&report.functions, opts.flagged_only);
    for func in &fns {
        lines.push(render_function(func, opts));
    }

    for cls in &report.classes {
        lines.extend(render_class_methods(cls, opts));
    }

    lines
}

pub fn render_file_report(report: &FileQualityReport, opts: &TextReporterOptions) -> String {
    let mut lines = render_file_header(report);

    if opts.scope == "file" {
        lines.push(render_file_summary_line(report));
        return lines.join("\n");
    }

    if opts.scope == "class" {
        lines.extend(render_class_scope(report, opts));
    } else {
        // scope: 'function' (default) — full per-function detail
        lines.extend(render_function_scope(report, opts));
    }

    lines.push(String::new());
    lines.push(render_file_summary_line(report));

    lines.join("\n")
}

// ─── Extracted helper: render project header ──────────────────────────────────

fn render_project_header(report: &ProjectQualityReport) -> Vec<String> {
    let s = &report.summary;

    vec![
        String::new(),
        format!("qualitas project: {}", report.dir_path)
            .bold()
            .to_string(),
        format!(
            "{}  score: {:.1}  grade: {}",
            score_bar(report.score),
            report.score,
            grade_color(report.grade, &report.grade.to_string()),
        ),
        String::new(),
        format!(
            "  {} files  |  {} functions  |  {}",
            s.total_files,
            s.total_functions,
            if s.flagged_functions > 0 {
                format!("{} need refactoring", s.flagged_functions)
                    .red()
                    .to_string()
            } else {
                format!("{} need refactoring", s.flagged_functions)
                    .green()
                    .to_string()
            },
        ),
        format!(
            "  Grades: {}  {}  {}  {}  {}",
            format!("A:{}", s.grade_distribution.a).green(),
            format!("B:{}", s.grade_distribution.b).cyan(),
            format!("C:{}", s.grade_distribution.c).yellow(),
            format!("D:{}", s.grade_distribution.d).red(),
            format!("F:{}", s.grade_distribution.f).white().on_red(),
        ),
    ]
}

// ─── Extracted helper: render worst functions section ─────────────────────────

fn format_worst_fn_line(func: &FunctionQualityReport) -> String {
    let icon = if func.needs_refactoring {
        "\u{2717}".red().to_string()
    } else {
        "\u{2713}".green().to_string()
    };
    format!(
        "    {icon} {}  {}()  score: {}  {}",
        func.location.file,
        func.name.bold(),
        func.score as u32,
        grade_color(func.grade, &func.grade.to_string()),
    )
}

fn render_worst_functions_section(
    report: &ProjectQualityReport,
    flagged_only: bool,
) -> Vec<String> {
    let funcs: Vec<&FunctionQualityReport> = if flagged_only {
        report
            .worst_functions
            .iter()
            .filter(|f| !f.flags.is_empty())
            .take(10)
            .collect()
    } else {
        report.worst_functions.iter().take(5).collect()
    };

    if funcs.is_empty() {
        return vec![];
    }

    let header = if flagged_only {
        "  Functions with flags:"
    } else {
        "  Worst functions:"
    };
    let mut lines = vec![String::new(), header.bold().to_string()];
    for func in &funcs {
        lines.push(format_worst_fn_line(func));
    }
    lines
}

fn file_has_flags(file: &FileQualityReport) -> bool {
    file.file_scope
        .as_ref()
        .is_some_and(|fs| !fs.flags.is_empty())
        || file.functions.iter().any(|f| !f.flags.is_empty())
        || file
            .classes
            .iter()
            .any(|c| c.methods.iter().any(|m| !m.flags.is_empty()))
}

// ─── Extracted helper: render file details section ────────────────────────────

fn render_file_details_section(
    report: &ProjectQualityReport,
    opts: &TextReporterOptions,
) -> Vec<String> {
    let mut lines = vec![String::new()];
    for file in &report.files {
        if opts.flagged_only && !file_has_flags(file) {
            continue;
        }
        lines.push(render_file_report(file, opts));
    }
    lines
}

// ─── Project report ───────────────────────────────────────────────────────────

pub fn render_project_report(report: &ProjectQualityReport, opts: &TextReporterOptions) -> String {
    let mut lines = render_project_header(report);

    if opts.scope == "module" {
        return lines.join("\n");
    }

    if opts.scope == "function" {
        lines.extend(render_worst_functions_section(report, opts.flagged_only));
    }

    lines.extend(render_file_details_section(report, opts));

    lines.join("\n")
}
