/// Language adapter trait and extraction types.
///
/// # Adding a new language
///
/// 1. Create `src/languages/<lang>.rs`
/// 2. Implement `LanguageAdapter` for your struct
/// 3. Register it in `src/languages/mod.rs`
///
/// That's it. The scorer, types, constants, and napi layer remain untouched.
///
/// **Key contract:** Import records must include binding names so per-function
/// coupling analysis works. See [`ImportRecord::names`] for details.
use crate::ir::events::QualitasEvent;

// ─── Extraction types ───────────────────────────────────────────────────────

/// Import record from file-level analysis.
///
/// **Important for adapter authors:** The `names` field must contain the local
/// binding names introduced by this import. These are used to determine which
/// imports each function actually references, enabling per-function coupling
/// analysis. If `names` is empty, the import will never be attributed to any
/// function and coupling scores will be artificially low.
///
/// Examples:
/// - `import { readFile, writeFile } from 'fs'` → names: `["readFile", "writeFile"]`
/// - `use std::collections::HashMap` → names: `["HashMap"]`
/// - `import fs from 'fs'` → names: `["fs"]`
#[derive(Debug, Clone)]
pub struct ImportRecord {
    /// Module specifier (e.g., "fs", "./utils", "lodash")
    pub source: String,
    /// Whether this is an external (non-relative) import
    pub is_external: bool,
    /// Local binding names introduced by this import. Must be populated for
    /// per-function coupling analysis to work correctly.
    pub names: Vec<String>,
}

/// Metadata + event stream for a single extracted function/method.
#[derive(Debug)]
pub struct FunctionExtraction {
    pub name: String,
    pub inferred_name: Option<String>,
    /// Byte offset of function start (used for LOC counting)
    pub byte_start: u32,
    /// Byte offset of function end (used for LOC counting)
    pub byte_end: u32,
    /// 1-based line number of function start (used in reports)
    pub start_line: u32,
    /// 1-based line number of function end (used in reports)
    pub end_line: u32,
    pub param_count: u32,
    pub is_async: bool,
    pub is_generator: bool,
    /// The stream of IR events for this function body.
    pub events: Vec<QualitasEvent>,
    /// Pre-computed LOC (used for file-scope where byte range is disjoint).
    pub loc_override: Option<u32>,
}

/// Metadata for a single extracted class/struct/module.
#[derive(Debug)]
pub struct ClassExtraction {
    pub name: String,
    /// Byte offset of class start
    pub byte_start: u32,
    /// Byte offset of class end
    pub byte_end: u32,
    /// 1-based line number of class start (used in reports)
    pub start_line: u32,
    /// 1-based line number of class end (used in reports)
    pub end_line: u32,
    pub methods: Vec<FunctionExtraction>,
}

/// Complete extraction result for one source file.
#[derive(Debug)]
pub struct FileExtraction {
    pub functions: Vec<FunctionExtraction>,
    pub classes: Vec<ClassExtraction>,
    pub imports: Vec<ImportRecord>,
    /// Top-level executable code analysis (control flow, expressions, try/catch).
    pub file_scope: Option<FunctionExtraction>,
}

// ─── Per-language threshold overrides ───────────────────────────────────────

/// Optional per-language overrides for normalization constants and flag thresholds.
/// Any `None` field falls back to the global default from `constants.rs`.
#[derive(Debug, Clone, Default)]
pub struct ThresholdOverrides {
    // Normalization constants (F-tier raw values)
    pub norm_cfc: Option<f64>,
    pub norm_dci_difficulty: Option<f64>,
    pub norm_dci_volume: Option<f64>,
    pub norm_irc: Option<f64>,
    pub norm_sm_loc: Option<f64>,
    pub norm_sm_params: Option<f64>,
    pub norm_sm_nesting: Option<f64>,
    pub norm_sm_returns: Option<f64>,

    // Flag thresholds (warning / error)
    pub cfc_warning: Option<u32>,
    pub cfc_error: Option<u32>,
    pub loc_warning: Option<u32>,
    pub loc_error: Option<u32>,
    pub params_warning: Option<u32>,
    pub params_error: Option<u32>,
    pub nesting_warning: Option<u32>,
    pub nesting_error: Option<u32>,
    pub returns_warning: Option<u32>,
    pub returns_error: Option<u32>,
}

// ─── Language adapter trait ─────────────────────────────────────────────────

/// The primary trait that language adapters implement.
///
/// A language adapter is responsible for:
/// 1. Parsing source code using whatever parser is best for the language
/// 2. Extracting function/class boundaries
/// 3. Walking each function body and emitting `QualitasEvent`s
///
/// The metric collectors, scorer, and output types are shared across all languages.
pub trait LanguageAdapter: Send + Sync {
    /// Human-readable name (e.g., "TypeScript", "Python", "Go").
    fn name(&self) -> &str;

    /// File extensions this adapter handles (e.g., `&[".ts", ".tsx", ".js"]`).
    fn extensions(&self) -> &[&str];

    /// Parse source text and extract all functions, classes, and imports.
    ///
    /// Each extracted function contains a `Vec<QualitasEvent>` representing
    /// the metric-relevant events found in its body.
    ///
    /// **Import records** must include binding names (`ImportRecord.names`) so
    /// the dependency coupling metric can determine which imports each function
    /// actually uses. See [`ImportRecord`] for details.
    fn extract(&self, source: &str, file_path: &str) -> Result<FileExtraction, String>;

    /// File name patterns that indicate test files for this language.
    ///
    /// Used by the CLI to skip test files unless `--include-tests` is passed.
    /// Patterns are matched as substrings against the file name.
    /// Override this to provide language-specific test conventions.
    fn test_patterns(&self) -> &[&str] {
        &[]
    }

    /// Return language-specific threshold overrides, or `None` to use defaults.
    ///
    /// Override this to adjust thresholds for your language. For example,
    /// Python functions tend to be shorter, so you might lower the LOC thresholds.
    fn threshold_overrides(&self) -> Option<ThresholdOverrides> {
        None
    }
}
