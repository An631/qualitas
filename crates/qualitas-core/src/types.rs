use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ─── Source location ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
}

// ─── Per-metric result types ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CognitiveFlowResult {
    /// Total CFC score
    pub score: u32,
    /// Sum of nesting penalties applied
    pub nesting_penalty: u32,
    /// Base increments (no nesting bonus)
    pub base_increments: u32,
    /// JS/TS-specific increments (Promise chains, nested callbacks, await)
    pub async_penalty: u32,
    /// Maximum nesting depth observed in this scope
    pub max_nesting_depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HalsteadCounts {
    pub distinct_operators: u32,
    pub distinct_operands: u32,
    pub total_operators: u32,
    pub total_operands: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataComplexityResult {
    pub halstead: HalsteadCounts,
    /// D = (η1/2) × (N2/η2)
    pub difficulty: f64,
    /// V = N × log2(η)
    pub volume: f64,
    /// E = D × V
    pub effort: f64,
    /// Composite normalized 0–∞ (used for scoring)
    pub raw_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifierHotspot {
    pub name: String,
    pub reference_count: u32,
    pub definition_line: u32,
    pub last_reference_line: u32,
    pub scope_span_lines: u32,
    /// cost = `reference_count` × `log2(scope_span_lines` + 1)
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifierRefResult {
    /// Sum of all identifier costs in this scope
    pub total_irc: f64,
    /// Top-10 most expensive identifiers
    pub hotspots: Vec<IdentifierHotspot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyCouplingResult {
    pub import_count: u32,
    pub distinct_sources: u32,
    /// Fraction of imports from `node_modules` (0.0–1.0)
    pub external_ratio: f64,
    pub external_packages: Vec<String>,
    pub internal_modules: Vec<String>,
    /// Distinct module-qualified API calls inside a function
    pub distinct_api_calls: u32,
    /// Outer-scope identifiers captured in function closure
    pub closure_captures: u32,
    /// Composite normalized 0–∞
    pub raw_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuralResult {
    pub loc: u32,
    pub total_lines: u32,
    pub parameter_count: u32,
    pub max_nesting_depth: u32,
    pub return_count: u32,
    pub method_count: Option<u32>,
    /// Composite normalized 0–∞
    pub raw_score: f64,
}

// ─── Composite metric container ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricBreakdown {
    pub cognitive_flow: CognitiveFlowResult,
    pub data_complexity: DataComplexityResult,
    pub identifier_reference: IdentifierRefResult,
    pub dependency_coupling: DependencyCouplingResult,
    pub structural: StructuralResult,
}

// ─── Grade & score ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grade {
    A,
    B,
    C,
    D,
    F,
}

impl Grade {
    /// Returns a 0-based index for array-based grade distribution tallying.
    /// A=0, B=1, C=2, D=3, F=4
    pub fn index(self) -> usize {
        match self {
            Grade::A => 0,
            Grade::B => 1,
            Grade::C => 2,
            Grade::D => 3,
            Grade::F => 4,
        }
    }
}

impl std::fmt::Display for Grade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Grade::A => write!(f, "A"),
            Grade::B => write!(f, "B"),
            Grade::C => write!(f, "C"),
            Grade::D => write!(f, "D"),
            Grade::F => write!(f, "F"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreBreakdown {
    pub cfc_penalty: f64,
    pub dci_penalty: f64,
    pub irc_penalty: f64,
    pub dc_penalty: f64,
    pub sm_penalty: f64,
    pub total_penalty: f64,
}

// ─── Refactoring flags ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlagType {
    HighCognitiveFlow,
    HighDataComplexity,
    HighIdentifierChurn,
    TooManyParams,
    TooLong,
    DeepNesting,
    HighCoupling,
    ExcessiveReturns,
    HighHalsteadEffort,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefactoringFlag {
    pub flag_type: FlagType,
    pub severity: Severity,
    pub message: String,
    pub suggestion: String,
    pub observed_value: f64,
    pub threshold: f64,
}

// ─── Report types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionQualityReport {
    pub name: String,
    pub inferred_name: Option<String>,
    /// Quality Score 0–100, higher = better
    pub score: f64,
    pub grade: Grade,
    pub needs_refactoring: bool,
    pub flags: Vec<RefactoringFlag>,
    pub metrics: MetricBreakdown,
    pub score_breakdown: ScoreBreakdown,
    pub location: SourceLocation,
    pub is_async: bool,
    pub is_generator: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassQualityReport {
    pub name: String,
    pub score: f64,
    pub grade: Grade,
    pub needs_refactoring: bool,
    pub flags: Vec<RefactoringFlag>,
    pub structural_metrics: StructuralResult,
    pub methods: Vec<FunctionQualityReport>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileQualityReport {
    pub file_path: String,
    pub score: f64,
    pub grade: Grade,
    pub needs_refactoring: bool,
    pub flags: Vec<RefactoringFlag>,
    pub functions: Vec<FunctionQualityReport>,
    pub classes: Vec<ClassQualityReport>,
    pub file_dependencies: DependencyCouplingResult,
    pub total_lines: u32,
    pub function_count: u32,
    pub class_count: u32,
    pub flagged_function_count: u32,
}

// TODO: construct these from Rust when analyze_project is moved to the native layer.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub total_files: u32,
    pub total_functions: u32,
    pub total_classes: u32,
    pub flagged_files: u32,
    pub flagged_functions: u32,
    pub average_score: f64,
    pub grade_distribution: GradeDistribution,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradeDistribution {
    pub a: u32,
    pub b: u32,
    pub c: u32,
    pub d: u32,
    pub f: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectQualityReport {
    pub dir_path: String,
    pub score: f64,
    pub grade: Grade,
    pub needs_refactoring: bool,
    pub files: Vec<FileQualityReport>,
    pub summary: ProjectSummary,
    /// Top 10 worst-scoring functions across the project
    pub worst_functions: Vec<FunctionQualityReport>,
}

// ─── Flag configuration ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlagConfig {
    Enabled(bool),
    Custom { warn: f64, error: f64 },
}

// ─── Configuration types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightConfig {
    pub cognitive_flow: f64,
    pub data_complexity: f64,
    pub identifier_reference: f64,
    pub dependency_coupling: f64,
    pub structural: f64,
}

impl Default for WeightConfig {
    fn default() -> Self {
        Self {
            cognitive_flow: 0.30,
            data_complexity: 0.25,
            identifier_reference: 0.20,
            dependency_coupling: 0.15,
            structural: 0.10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisOptions {
    pub profile: Option<String>,
    pub weights: Option<WeightConfig>,
    pub refactoring_threshold: Option<f64>,
    pub include_tests: Option<bool>,
    pub extensions: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub flag_overrides: Option<HashMap<String, FlagConfig>>,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            profile: None,
            weights: None,
            refactoring_threshold: Some(65.0),
            include_tests: Some(false),
            extensions: None,
            exclude: None,
            flag_overrides: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QualitasConfig {
    pub threshold: Option<f64>,
    pub profile: Option<String>,
    pub format: Option<String>,
    pub include_tests: Option<bool>,
    pub exclude: Option<Vec<String>>,
    pub extensions: Option<Vec<String>>,
    pub weights: Option<WeightConfig>,
    pub flags: Option<HashMap<String, FlagConfig>>,
    pub languages: Option<HashMap<String, LanguageConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LanguageConfig {
    pub test_patterns: Option<Vec<String>>,
    pub flags: Option<HashMap<String, FlagConfig>>,
}
