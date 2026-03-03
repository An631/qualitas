use colored::Colorize;

use qualitas_core::types::{
    FileQualityReport, FunctionQualityReport, Grade, ProjectQualityReport, RefactoringFlag,
    Severity,
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

fn score_bar(score: f64) -> String {
    let filled = (score / 10.0).round() as usize;
    let filled = filled.min(10);
    let bar = format!(
        "{}{}",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(10 - filled)
    );
    grade_color(score_to_grade(score), &bar)
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
            score_to_grade(100.0 - f64::from(m.cognitive_flow.score) * 4.0),
            m.data_complexity.difficulty,
            score_to_grade(100.0 - m.data_complexity.difficulty),
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

pub fn render_file_report(report: &FileQualityReport, opts: &TextReporterOptions) -> String {
    let mut lines: Vec<String> = Vec::new();
    let grade_str = grade_color(report.grade, &report.grade.to_string());

    lines.push(String::new());
    lines.push(format!("qualitas: {}", report.file_path).bold().to_string());
    lines.push(format!(
        "{}  score: {:.1}  grade: {grade_str}",
        score_bar(report.score),
        report.score,
    ));
    lines.push(String::new());

    if opts.scope == "file" {
        lines.push(render_file_summary_line(report));
        return lines.join("\n");
    }

    if opts.scope == "class" {
        for cls in &report.classes {
            if opts.flagged_only && !cls.needs_refactoring {
                continue;
            }
            lines.push(format!(
                "  {}  {}  score: {:.1}",
                format!("class {}", cls.name).cyan(),
                grade_color(cls.grade, &cls.grade.to_string()),
                cls.score,
            ));
            if !cls.flags.is_empty() {
                lines.push(format!("    {}", "Flags:".dimmed()));
                for flag in &cls.flags {
                    lines.push(render_flag(flag, "    "));
                }
            }
        }
    } else {
        // scope: 'function' (default) — full per-function detail
        let fns: Vec<&FunctionQualityReport> = if opts.flagged_only {
            report
                .functions
                .iter()
                .filter(|f| f.needs_refactoring)
                .collect()
        } else {
            report.functions.iter().collect()
        };

        for func in &fns {
            lines.push(render_function(func, opts));
        }

        for cls in &report.classes {
            let class_fns: Vec<&FunctionQualityReport> = if opts.flagged_only {
                cls.methods.iter().filter(|m| m.needs_refactoring).collect()
            } else {
                cls.methods.iter().collect()
            };

            if opts.flagged_only && class_fns.is_empty() {
                continue;
            }

            lines.push(String::new());
            lines.push(format!(
                "  {}  {}  score: {:.1}",
                format!("class {}", cls.name).cyan(),
                grade_color(cls.grade, &cls.grade.to_string()),
                cls.score,
            ));
            for m in &class_fns {
                lines.push(render_function(m, opts));
            }
        }
    }

    lines.push(String::new());
    lines.push(render_file_summary_line(report));

    lines.join("\n")
}

// ─── Project report ───────────────────────────────────────────────────────────

pub fn render_project_report(report: &ProjectQualityReport, opts: &TextReporterOptions) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(String::new());
    lines.push(
        format!("qualitas project: {}", report.dir_path)
            .bold()
            .to_string(),
    );
    lines.push(format!(
        "{}  score: {:.1}  grade: {}",
        score_bar(report.score),
        report.score,
        grade_color(report.grade, &report.grade.to_string()),
    ));
    lines.push(String::new());

    let s = &report.summary;
    lines.push(format!(
        "  {} files  |  {} functions  |  {}",
        s.total_files,
        s.total_functions,
        format!("{} need refactoring", s.flagged_functions).red(),
    ));
    lines.push(format!(
        "  Grades: {}  {}  {}  {}  {}",
        format!("A:{}", s.grade_distribution.a).green(),
        format!("B:{}", s.grade_distribution.b).cyan(),
        format!("C:{}", s.grade_distribution.c).yellow(),
        format!("D:{}", s.grade_distribution.d).red(),
        format!("F:{}", s.grade_distribution.f).white().on_red(),
    ));

    if opts.scope == "module" {
        return lines.join("\n");
    }

    if !report.worst_functions.is_empty() && opts.scope == "function" {
        lines.push(String::new());
        lines.push("  Worst functions:".bold().to_string());
        for func in report.worst_functions.iter().take(5) {
            lines.push(format!(
                "    {} {}  {}()  score: {}  {}",
                "\u{2717}".red(),
                func.location.file,
                func.name.bold(),
                func.score as u32,
                grade_color(func.grade, &func.grade.to_string()),
            ));
        }
    }

    if opts.verbose || !opts.flagged_only {
        lines.push(String::new());
        for file in &report.files {
            if opts.flagged_only && !file.needs_refactoring {
                continue;
            }
            lines.push(render_file_report(file, opts));
        }
    }

    lines.join("\n")
}
