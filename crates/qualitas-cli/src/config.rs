use std::path::Path;
use std::process::Command;

use qualitas_core::types::{AnalysisOptions, QualitasConfig};

/// Walk up directories from `start_dir` looking for `qualitas.config.js`.
/// When found, evaluate it with Node and parse the JSON output.
/// Returns `QualitasConfig::default()` if the file is not found, Node is
/// unavailable, or the output cannot be parsed.
pub fn load_config(start_dir: &str) -> QualitasConfig {
    find_config_file(start_dir)
        .map(|path| evaluate_config(&path))
        .unwrap_or_default()
}

fn find_config_file(start_dir: &str) -> Option<std::path::PathBuf> {
    let start = Path::new(start_dir);
    let mut dir = if start.is_file() {
        start.parent()?
    } else {
        start
    };

    loop {
        let candidate = dir.join("qualitas.config.js");
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

/// Run the config file through Node and parse the JSON result.
fn evaluate_config(config_path: &Path) -> QualitasConfig {
    // Get the absolute (canonical) path so the require() call works regardless of cwd
    let Ok(abs_path) = config_path.canonicalize() else {
        return QualitasConfig::default();
    };

    // Use forward slashes in the path for the JS require() call (works on all platforms)
    let path_str = abs_path.to_string_lossy().replace('\\', "/");

    // Strip the UNC prefix \\?\ that Windows canonicalize adds
    let path_str = path_str
        .strip_prefix("//\\?/")
        .or_else(|| path_str.strip_prefix("//\\\\?\\\\"))
        .unwrap_or(&path_str);
    let path_str = path_str.strip_prefix("//?/").unwrap_or(path_str);

    let script = format!("console.log(JSON.stringify(require(\"{path_str}\")))");

    let Ok(output) = Command::new("node").args(["-e", &script]).output() else {
        return QualitasConfig::default(); // Node not available
    };

    if !output.status.success() {
        return QualitasConfig::default();
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_default()
}

/// Merge CLI arguments with the loaded config file, using CLI > config > defaults.
/// Returns `(AnalysisOptions, format_string)`.
/// `TextReporterOptions` is now derived from the format string in main, not here.
pub fn merge_config(cli: &super::Cli, config: &QualitasConfig) -> (AnalysisOptions, String) {
    let format = resolve_string(cli.format.as_ref(), config.format.as_ref(), "text");
    let options = build_analysis_options(cli, config);
    (options, format)
}

fn resolve_string(cli_val: Option<&String>, config_val: Option<&String>, default: &str) -> String {
    cli_val
        .or(config_val)
        .map_or_else(|| default.to_string(), String::clone)
}

fn resolve_bool(cli_val: bool, config_val: Option<bool>) -> bool {
    if cli_val {
        true
    } else {
        config_val.unwrap_or(false)
    }
}

fn build_analysis_options(cli: &super::Cli, config: &QualitasConfig) -> AnalysisOptions {
    let profile = cli.profile.clone().or_else(|| config.profile.clone());
    let threshold = cli.threshold.or(config.threshold).unwrap_or(65.0);

    AnalysisOptions {
        profile: profile.as_deref().and_then(|p| {
            if p == "default" {
                None
            } else {
                Some(p.to_string())
            }
        }),
        weights: config.weights.clone(),
        refactoring_threshold: Some(threshold),
        include_tests: Some(resolve_bool(cli.include_tests, config.include_tests)),
        extensions: config.extensions.clone(),
        exclude: config.exclude.clone(),
    }
}
