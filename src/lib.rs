#![deny(clippy::all)]

mod analyzer;
mod constants;
pub mod ir;
mod languages;
mod metrics;
mod parser;
mod scorer;
mod types;

use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Analyze source code and return a FileQualityReport as JSON.
///
/// Language is auto-detected from the `file_name` extension.
/// Returns a JSON-serialized `FileQualityReport`.
#[napi]
pub fn analyze_source(source: String, file_name: String, options_json: Option<String>) -> Result<String> {
    let options: types::AnalysisOptions = options_json
        .as_deref()
        .map(|s| serde_json::from_str(s).unwrap_or_default())
        .unwrap_or_default();

    let report = analyzer::analyze_source_str(&source, &file_name, &options)
        .map_err(|e| napi::Error::from_reason(e))?;

    serde_json::to_string(&report)
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}

/// List all supported languages and their file extensions.
///
/// Returns a JSON array of `{ name, extensions }` objects.
#[napi]
pub fn supported_languages() -> String {
    let adapters = languages::list_adapters();
    let langs: Vec<serde_json::Value> = adapters
        .iter()
        .map(|a| {
            serde_json::json!({
                "name": a.name(),
                "extensions": a.extensions(),
            })
        })
        .collect();
    serde_json::to_string(&langs).unwrap_or_else(|_| "[]".to_string())
}

/// Analyze a TypeScript/JavaScript source string and return a compact summary.
/// Faster than `analyze_source` when you only need score + grade + flags.
#[napi]
pub fn quick_score(source: String, file_name: String) -> Result<String> {
    let options = types::AnalysisOptions::default();
    let report = analyzer::analyze_source_str(&source, &file_name, &options)
        .map_err(|e| napi::Error::from_reason(e))?;

    let summary = serde_json::json!({
        "score": report.score,
        "grade": report.grade,
        "needsRefactoring": report.needs_refactoring,
        "functionCount": report.function_count,
        "flaggedFunctionCount": report.flagged_function_count,
        "topFlags": report.functions.iter()
            .flat_map(|f| f.flags.iter())
            .take(5)
            .collect::<Vec<_>>(),
    });

    serde_json::to_string(&summary)
        .map_err(|e| napi::Error::from_reason(e.to_string()))
}
