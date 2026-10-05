use std::path::{Path, PathBuf};
use std::process::Command;

use qualitas_core::types::{AnalysisOptions, QualitasConfig};

/// Load configuration from a qualitas.config file.
///
/// Search order:
/// 1. Explicit `--config` path (if provided)
/// 2. Walk up from `start_dir` looking for qualitas.config.js/.cjs/.mjs
/// 3. Look next to the running executable
///
/// Returns `QualitasConfig::default()` if no config file is found.
pub fn load_config(start_dir: &str, explicit_path: Option<&str>) -> Result<QualitasConfig, String> {
    match find_config(start_dir, explicit_path)? {
        Some(path) => evaluate_config(&path),
        None => Ok(QualitasConfig::default()),
    }
}

fn find_config(start_dir: &str, explicit_path: Option<&str>) -> Result<Option<PathBuf>, String> {
    // 1. Explicit --config flag takes priority
    if let Some(path) = explicit_path {
        let p = Path::new(path);
        if p.is_file() {
            return Ok(Some(p.to_path_buf()));
        }
        return Err(format!("config file not found: {path}"));
    }

    // 2. Walk up from target directory
    if let Some(found) = walk_up_for_config(start_dir) {
        return Ok(Some(found));
    }

    // 3. Look next to the executable
    Ok(find_config_next_to_exe())
}

fn walk_up_for_config(start_dir: &str) -> Option<std::path::PathBuf> {
    let start = Path::new(start_dir);
    let mut dir = if start.is_file() {
        start.parent()?
    } else {
        start
    };

    loop {
        for filename in [
            "qualitas.config.js",
            "qualitas.config.cjs",
            "qualitas.config.mjs",
        ] {
            let candidate = dir.join(filename);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        dir = dir.parent()?;
    }
}

fn find_config_next_to_exe() -> Option<std::path::PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    for filename in [
        "qualitas.config.js",
        "qualitas.config.cjs",
        "qualitas.config.mjs",
    ] {
        let candidate = exe_dir.join(filename);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Dynamically import the config through Node so CommonJS and ESM are supported.
fn evaluate_config(config_path: &Path) -> Result<QualitasConfig, String> {
    let abs_path = config_path.canonicalize().map_err(|error| {
        format!(
            "failed to resolve config file {}: {error}",
            config_path.display()
        )
    })?;

    let script = r"
import('node:url').then(({ pathToFileURL }) =>
  import(pathToFileURL(process.argv[1]).href)
).then((module) => {
  const config = Object.hasOwn(module, 'default') ? module.default : module;
  if (!config || typeof config !== 'object' || Array.isArray(config)) {
    throw new Error('Config must export an object');
  }
  const json = JSON.stringify(config);
  if (json === undefined) throw new Error('Config could not be serialized');
  process.stdout.write('__QUALITAS_CONFIG__' + json + '\n');
}).catch((error) => {
  console.error(error.stack || error);
  process.exitCode = 1;
});
";

    let output = Command::new("node")
        .args(["-e", script])
        .arg(&abs_path)
        .output()
        .map_err(|error| {
            format!(
                "failed to run Node.js to load {}: {error}",
                config_path.display()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "failed to load config file {}: {}",
            config_path.display(),
            stderr.trim()
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json = stdout
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("__QUALITAS_CONFIG__"))
        .ok_or_else(|| {
            format!(
                "failed to load config file {}: Node.js returned no config data",
                config_path.display()
            )
        })?;
    serde_json::from_str(json).map_err(|error| {
        format!(
            "failed to parse config file {}: {error}",
            config_path.display()
        )
    })
}

/// Merge CLI arguments with the loaded config file, using CLI > config > defaults.
/// Returns `(AnalysisOptions, format_string)`.
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
        flag_overrides: None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{evaluate_config, find_config, load_config};

    fn temp_dir() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "qualitas-config-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn imports_default_export_from_esm_config() {
        let dir = temp_dir();
        fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
        let config_path = dir.join("qualitas.config.js");
        fs::write(
            &config_path,
            "export default { threshold: 90, exclude: ['node_modules'] };",
        )
        .unwrap();

        let config = evaluate_config(&config_path).unwrap();
        assert_eq!(config.threshold, Some(90.0));
        assert_eq!(config.exclude, Some(vec!["node_modules".to_string()]));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_commonjs_config_with_cjs_extension() {
        let dir = temp_dir();
        let config_path = dir.join("qualitas.config.cjs");
        fs::write(&config_path, "module.exports = { threshold: 80 };").unwrap();

        let config = evaluate_config(&config_path).unwrap();
        assert_eq!(config.threshold, Some(80.0));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_config_load_errors() {
        let dir = temp_dir();
        let config_path = dir.join("qualitas.config.mjs");
        fs::write(&config_path, "export default { threshold: ;").unwrap();

        let error = evaluate_config(&config_path).unwrap_err();
        assert!(error.contains("failed to load config file"));
        assert!(error.contains("qualitas.config.mjs"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn discovers_cjs_and_mjs_config_files() {
        let dir = temp_dir();
        fs::write(dir.join("qualitas.config.cjs"), "module.exports = {};").unwrap();
        assert_eq!(
            find_config(dir.to_str().unwrap(), None).unwrap(),
            Some(dir.join("qualitas.config.cjs"))
        );
        fs::remove_file(dir.join("qualitas.config.cjs")).unwrap();
        fs::write(dir.join("qualitas.config.mjs"), "export default {};").unwrap();
        assert_eq!(
            find_config(dir.to_str().unwrap(), None).unwrap(),
            Some(dir.join("qualitas.config.mjs"))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_missing_config_is_an_error() {
        let error = load_config(".", Some("missing-qualitas.config.js")).unwrap_err();
        assert!(error.contains("config file not found"));
    }
}
