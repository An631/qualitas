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

    let script = r#"
import('node:url').then(({ pathToFileURL }) =>
  import(pathToFileURL(process.argv[1]).href)
).then((module) => {
  if (Object.keys(module).length === 0) {
    throw new Error(
      'Config exported nothing. In a "type": "module" package, use ' +
      '`export default { ... }` or rename the file to qualitas.config.cjs ' +
      'if it uses `module.exports`.'
    );
  }
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
"#;

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
    for warning in config_warnings(json) {
        eprintln!("warning: {}: {warning}", config_path.display());
    }
    serde_json::from_str(json).map_err(|error| {
        format!(
            "failed to parse config file {}: {error}",
            config_path.display()
        )
    })
}

const KNOWN_KEYS: [&str; 10] = [
    "threshold",
    "profile",
    "format",
    "includeTests",
    "exclude",
    "extensions",
    "weights",
    "flags",
    "languages",
    "failOnFlags",
];

/// Describe config problems that would otherwise be silently ignored:
/// unrecognised keys (with a suggestion) and configs with no recognised keys.
fn config_warnings(json: &str) -> Vec<String> {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };

    let mut warnings = Vec::new();
    let mut recognised = 0;
    for key in map.keys() {
        if KNOWN_KEYS.contains(&key.as_str()) {
            recognised += 1;
            continue;
        }
        let hint = suggest_key(key)
            .map(|known| format!(" (did you mean `{known}`?)"))
            .unwrap_or_default();
        warnings.push(format!("unknown config key `{key}`{hint}"));
    }
    if recognised == 0 {
        warnings.push(format!(
            "config has no recognised keys, defaults will be used (valid keys: {})",
            KNOWN_KEYS.join(", ")
        ));
    }
    warnings
}

fn suggest_key(key: &str) -> Option<&'static str> {
    let lower = key.to_lowercase();
    KNOWN_KEYS
        .iter()
        .map(|known| (*known, edit_distance(&lower, &known.to_lowercase())))
        .filter(|(_, distance)| *distance <= 2)
        .min_by_key(|(_, distance)| *distance)
        .map(|(known, _)| known)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        row = next_row(&row, ca, &b, i + 1);
    }
    row[b.len()]
}

fn next_row(prev: &[usize], ca: char, b: &[char], first: usize) -> Vec<usize> {
    let mut row = vec![first];
    for (j, cb) in b.iter().enumerate() {
        let cost = usize::from(ca != *cb);
        row.push((prev[j + 1] + 1).min(row[j] + 1).min(prev[j] + cost));
    }
    row
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

    use super::{config_warnings, evaluate_config, find_config, load_config};

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
    fn commonjs_exports_in_esm_package_are_an_error() {
        let dir = temp_dir();
        fs::write(dir.join("package.json"), r#"{"type":"module"}"#).unwrap();
        let config_path = dir.join("qualitas.config.js");
        fs::write(&config_path, "module.exports = { threshold: 90 };").unwrap();

        let error = evaluate_config(&config_path).unwrap_err();
        assert!(error.contains("Config exported nothing"));
        assert!(error.contains("qualitas.config.cjs"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_empty_default_export_is_allowed() {
        let dir = temp_dir();
        let config_path = dir.join("qualitas.config.mjs");
        fs::write(&config_path, "export default {};").unwrap();

        assert!(evaluate_config(&config_path).is_ok());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn warns_about_typos_with_suggestion() {
        let warnings = config_warnings(r#"{"treshold":90,"excludes":[]}"#);
        assert!(warnings
            .iter()
            .any(|w| w.contains("`treshold`") && w.contains("`threshold`")));
        assert!(warnings
            .iter()
            .any(|w| w.contains("`excludes`") && w.contains("`exclude`")));
        assert!(warnings.iter().any(|w| w.contains("no recognised keys")));
    }

    #[test]
    fn warns_when_config_is_empty() {
        let warnings = config_warnings("{}");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("no recognised keys"));
    }

    #[test]
    fn valid_config_has_no_warnings() {
        assert!(config_warnings(r#"{"threshold":90,"exclude":["a"]}"#).is_empty());
    }

    #[test]
    fn warns_about_unknown_key_alongside_valid_ones() {
        let warnings = config_warnings(r#"{"threshold":90,"bogusthing":1}"#);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("`bogusthing`") && !warnings[0].contains("did you mean"));
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
