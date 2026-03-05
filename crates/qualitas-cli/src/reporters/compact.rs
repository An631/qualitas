use colored::Colorize;

use qualitas_core::types::{FileQualityReport, Grade, ProjectQualityReport};

// ─── Public entry points ─────────────────────────────────────────────────────

pub fn render_compact_file(report: &FileQualityReport) -> String {
    let flags = count_all_flags(report);
    format_compact_line(
        &short_name(&report.file_path),
        report.score,
        report.grade,
        flags,
    )
}

pub fn render_compact_project(report: &ProjectQualityReport) -> String {
    let mut files: Vec<&FileQualityReport> = report.files.iter().collect();
    files.sort_by(|a, b| {
        a.score
            .partial_cmp(&b.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines = vec![render_compact_header(report)];

    for file in &files {
        let flags = count_all_flags(file);
        lines.push(format_compact_line(
            &short_name(&file.file_path),
            file.score,
            file.grade,
            flags,
        ));
    }

    lines.join("\n")
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn render_compact_header(report: &ProjectQualityReport) -> String {
    let s = &report.summary;
    format!(
        "{}  score: {:.1}  grade: {}  files: {}  flagged: {}",
        "qualitas".bold(),
        report.score,
        grade_color(report.grade, &report.grade.to_string()),
        s.total_files,
        s.flagged_files,
    )
}

fn format_compact_line(name: &str, score: f64, grade: Grade, flags: usize) -> String {
    let grade_str = grade_color(grade, &grade.to_string());
    let score_str = format!("{score:>5.1}");
    let flag_str = if flags > 0 {
        format!("  {flags} flags").red().to_string()
    } else {
        String::new()
    };
    format!("  {grade_str}  {score_str}  {name}{flag_str}")
}

fn short_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn count_all_flags(report: &FileQualityReport) -> usize {
    let mut n = report.flags.len();
    if let Some(fs) = &report.file_scope {
        n += fs.flags.len();
    }
    for f in &report.functions {
        n += f.flags.len();
    }
    for cls in &report.classes {
        for m in &cls.methods {
            n += m.flags.len();
        }
    }
    n
}

fn grade_color(grade: Grade, text: &str) -> String {
    match grade {
        Grade::A => text.green().to_string(),
        Grade::B => text.cyan().to_string(),
        Grade::C => text.yellow().to_string(),
        Grade::D => text.red().to_string(),
        Grade::F => text.white().on_red().to_string(),
    }
}
