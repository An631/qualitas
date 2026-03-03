use qualitas_core::types::{FileQualityReport, ProjectQualityReport};

pub fn render_file_json(report: &FileQualityReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

pub fn render_project_json(report: &ProjectQualityReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}
