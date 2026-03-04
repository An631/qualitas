// TypeScript types mirroring the Rust serde structs.
// These are the shapes returned by the native binding.

export interface SourceLocation {
  file: string;
  startLine: number;
  endLine: number;
  startCol: number;
  endCol: number;
}

export interface HalsteadCounts {
  distinctOperators: number;
  distinctOperands: number;
  totalOperators: number;
  totalOperands: number;
}

export interface CognitiveFlowResult {
  score: number;
  nestingPenalty: number;
  baseIncrements: number;
  asyncPenalty: number;
  maxNestingDepth: number;
}

export interface DataComplexityResult {
  halstead: HalsteadCounts;
  difficulty: number;
  volume: number;
  effort: number;
  rawScore: number;
}

export interface IdentifierHotspot {
  name: string;
  referenceCount: number;
  definitionLine: number;
  lastReferenceLine: number;
  scopeSpanLines: number;
  cost: number;
}

export interface IdentifierRefResult {
  totalIrc: number;
  hotspots: IdentifierHotspot[];
}

export interface DependencyCouplingResult {
  importCount: number;
  distinctSources: number;
  externalRatio: number;
  externalPackages: string[];
  internalModules: string[];
  distinctApiCalls: number;
  closureCaptures: number;
  rawScore: number;
}

export interface StructuralResult {
  loc: number;
  totalLines: number;
  parameterCount: number;
  maxNestingDepth: number;
  returnCount: number;
  methodCount?: number;
  rawScore: number;
}

export interface MetricBreakdown {
  cognitiveFlow: CognitiveFlowResult;
  dataComplexity: DataComplexityResult;
  identifierReference: IdentifierRefResult;
  dependencyCoupling: DependencyCouplingResult;
  structural: StructuralResult;
}

export interface ScoreBreakdown {
  cfcPenalty: number;
  dciPenalty: number;
  ircPenalty: number;
  dcPenalty: number;
  smPenalty: number;
  totalPenalty: number;
}

export type Grade = 'A' | 'B' | 'C' | 'D' | 'F';

export type FlagType =
  | 'HIGH_COGNITIVE_FLOW'
  | 'HIGH_DATA_COMPLEXITY'
  | 'HIGH_IDENTIFIER_CHURN'
  | 'TOO_MANY_PARAMS'
  | 'TOO_LONG'
  | 'DEEP_NESTING'
  | 'HIGH_COUPLING'
  | 'EXCESSIVE_RETURNS'
  | 'HIGH_HALSTEAD_EFFORT';

export type Severity = 'info' | 'warning' | 'error';

export interface RefactoringFlag {
  flagType: FlagType;
  severity: Severity;
  message: string;
  suggestion: string;
  observedValue: number;
  threshold: number;
}

export interface FunctionQualityReport {
  name: string;
  inferredName?: string;
  /** Quality Score 0–100, higher = better */
  score: number;
  grade: Grade;
  needsRefactoring: boolean;
  flags: RefactoringFlag[];
  metrics: MetricBreakdown;
  scoreBreakdown: ScoreBreakdown;
  location: SourceLocation;
  isAsync: boolean;
  isGenerator: boolean;
}

export interface ClassQualityReport {
  name: string;
  score: number;
  grade: Grade;
  needsRefactoring: boolean;
  flags: RefactoringFlag[];
  structuralMetrics: StructuralResult;
  methods: FunctionQualityReport[];
  location: SourceLocation;
}

export interface FileQualityReport {
  filePath: string;
  score: number;
  grade: Grade;
  needsRefactoring: boolean;
  flags: RefactoringFlag[];
  functions: FunctionQualityReport[];
  classes: ClassQualityReport[];
  fileDependencies: DependencyCouplingResult;
  totalLines: number;
  functionCount: number;
  classCount: number;
  flaggedFunctionCount: number;
}

export interface GradeDistribution {
  a: number;
  b: number;
  c: number;
  d: number;
  f: number;
}

export interface ProjectSummary {
  totalFiles: number;
  totalFunctions: number;
  totalClasses: number;
  flaggedFiles: number;
  flaggedFunctions: number;
  averageScore: number;
  gradeDistribution: GradeDistribution;
}

export interface ProjectQualityReport {
  dirPath: string;
  score: number;
  grade: Grade;
  needsRefactoring: boolean;
  files: FileQualityReport[];
  summary: ProjectSummary;
  worstFunctions: FunctionQualityReport[];
}

export interface WeightConfig {
  cognitiveFlow: number;
  dataComplexity: number;
  identifierReference: number;
  dependencyCoupling: number;
  structural: number;
}

export type ProfileName = 'default' | 'cc-focused' | 'data-focused' | 'strict';

export interface AnalysisOptions {
  profile?: ProfileName;
  weights?: Partial<WeightConfig>;
  refactoringThreshold?: number;
  includeTests?: boolean;
  extensions?: string[];
  exclude?: string[];
}

export interface QualitasConfig {
  threshold?: number;
  profile?: ProfileName;
  format?: string;
  includeTests?: boolean;
  exclude?: string[];
  extensions?: string[];
  weights?: Partial<WeightConfig>;
  languages?: Record<string, LanguageConfig>;
}

export interface LanguageConfig {
  testPatterns?: string[];
}

/** Compact result returned by `quickScore()` — faster than full `analyzeSource()`. */
export interface QuickScore {
  score: number;
  grade: Grade;
  needsRefactoring: boolean;
  functionCount: number;
  flaggedFunctionCount: number;
  topFlags: RefactoringFlag[];
}
