# Qualitas

**A next generation code quality measurement tool that actually works**. It measures code quality across five research-backed pillars and returns a single 0–100 **Quality Score** to guide the health of your code base. It supports TypeScript/JavaScript, Rust, and Python out of the box. The core is written in Rust using [oxc_parser](https://oxc.rs/) and [tree-sitter](https://tree-sitter.github.io/) for native-speed analysis, distributed as a native npm package via [napi-rs](https://napi.rs/), and provides both a programmatic TypeScript API and a CLI binary for ease of use.

---

## Why another complexity tool?

Cognitive Complexity (CC-Sonar, the metric behind SonarQube) is the leading code quality metric for modern languages, but a 2023 PMC study using EEG and eye-tracking found measurable gaps:

| Metric | Correlation with measured cognitive load |
|--------|------------------------------------------|
| Halstead Effort (DCI) | **r = 0.901** |
| Eye-tracking revisit count (IRC) | **r = 0.963** |
| CC-Sonar alone | r = 0.513 |

CC-Sonar only measures control flow structure. It misses:

- **Data complexity** — how many variables, operators, and distinct symbols the reader must track simultaneously (Halstead)
- **Identifier churn** — how often a reader must revisit a variable across a wide scope (eye-tracking research)
- **Saturation effect** — once code is complex enough, doubling raw CC-Sonar doesn't double perceived difficulty

`qualitas` addresses all three gaps through a five-pillar composite score with an exponential saturation model.

> PMC paper: *"Measuring cognitive complexity in software development"* — <https://pmc.ncbi.nlm.nih.gov/articles/PMC9942489/>

---

## Quick start

```bash
# Analyze a file (text output, default)
npx qualitas ./src/myFile.ts

# Analyze a directory
npx qualitas ./src/

# JSON report (writes to qualitas-report.json by default)
npx qualitas ./src/ -f json

# JSON report to a specific file
npx qualitas ./src/ -f json -o report.json

# Show only functions that have flags
npx qualitas ./src/ -f flagged

# Fail CI if any score is below threshold
npx qualitas ./src/ --threshold 65

# Zero-tolerance mode — fail on any warning flag
npx qualitas ./src/ --fail-on-flags warn
```

---

## Installation

```bash
npm install qualitas
```

The correct native binary for your platform is installed automatically as an optional dependency:

| Platform | Package |
|----------|---------|
| macOS Apple Silicon | `@qualitas/binding-darwin-arm64` |
| macOS Intel | `@qualitas/binding-darwin-x64` |
| Linux x64 (glibc) | `@qualitas/binding-linux-x64-gnu` |
| Linux arm64 (glibc) | `@qualitas/binding-linux-arm64-gnu` |
| Windows x64 | `@qualitas/binding-win32-x64-msvc` |

No build step required when installing from npm.

---

## The Five Pillars

### 1. Cognitive Flow Complexity (CFC) — 20% weight

Tracks nesting depth during AST traversal and applies progressively larger penalties for deeply nested control flow.

**Scoring increments:**

| AST Node | Increment | Nesting penalty? |
|----------|-----------|-----------------|
| `if` / `else if` | +1 | Yes (+nestingDepth) |
| `for`, `for...of`, `for...in` | +1 | Yes |
| `while`, `do...while` | +1 | Yes |
| `switch` | +1 | Yes |
| `catch` clause | +1 | Yes |
| `&&`, `\|\|`, `??` (logical) | +1 per operator | No |
| Ternary `?:` | +1 | No |
| Recursive call (self-reference) | +1 | No |
| `.then()` / `.catch()` chain | +1 | Yes (current depth) |
| Nested arrow callback | +nestingDepth | Yes |
| `await` inside nested function | +1 | Yes |

**Formula:** `CFC = Σ(1 + nestingDepth)` for nesting nodes, `+1` flat for non-nesting nodes.

---

### 2. Data Complexity Index (DCI) — 30% weight

Halstead-inspired metric. Addresses CC-Sonar's largest gap (r=0.901 correlation in the PMC paper vs CC-Sonar's r=0.513).

**Counting:**

- **Operators (η₁ distinct, N₁ total):** `+`, `-`, `*`, `/`, `===`, `!==`, `<`, `>`, `<=`, `>=`, `=`, `+=`, `-=`, `&&`, `||`, `??`, `!`, `typeof`, `instanceof`, `?.`, `??=`, etc.
- **Operands (η₂ distinct, N₂ total):** identifiers (variables/params), string/numeric/template literals, `this`, `null`, `undefined`, boolean literals

**Computed values:**

```text
Vocabulary  η = η₁ + η₂
Length      N = N₁ + N₂
Volume      V = N × log₂(η)
Difficulty  D = (η₁/2) × (N₂/η₂)
Effort      E = D × V
```

**Normalized raw score:**

```text
DCI_raw = 0.6 × (D / 60) + 0.4 × (V / 3000)
```

Where 60 = F-tier difficulty threshold, 3000 = F-tier volume threshold.

**Why it matters:** A function with 20 distinct variable names, 15 distinct operators, and 200 total token appearances forces the reader to hold a large mental vocabulary simultaneously. CC-Sonar scores this identically to a trivial function with the same branching structure.

---

### 3. Identifier Reference Complexity (IRC) — 25% weight

Novel metric. Inspired by the eye-tracking finding (r=0.963) that revisit count — how often a developer re-reads a variable while understanding code — is the strongest predictor of cognitive load.

**Algorithm:** For each declared identifier (variable, parameter, destructured binding) in function scope:

```text
cost = referenceCount × log₂(scopeSpanLines + 1)
     where scopeSpanLines = lastReferenceLine − definitionLine
```

**Total IRC = Σ(cost)** over all identifiers.

**Why it matters:** A variable `x` referenced once on the line after it's declared has cost ≈ 1. A variable `config` referenced 12 times across a 150-line function has cost ≈ 12 × log₂(151) ≈ 87. The log₂ damping prevents enormous scope spans from completely dominating — matching the paper's finding that saturation occurs.

---

### 4. Dependency Coupling (DC) — 15% weight

Measures how many external dependencies and distinct APIs a function/file touches.

**At file level:**

- `importCount` — total import statements
- `distinctSources` — unique module specifiers
- `externalRatio` — imports from `node_modules` / total imports

**At function level:**

- `distinctApiCalls` — distinct imported-module methods called (e.g., `fs.readFile`, `axios.get` = 2)
- `closureCaptures` — identifiers from outer scope referenced inside

**Normalized raw score:**

```text
DC_raw = 0.4 × (importCount / 20) + 0.3 × externalRatio + 0.3 × (distinctApiCalls / 15)
```

---

### 5. Structural Metrics (SM) — 10% weight

Count-based metrics that catch simple but impactful structural issues.

- `loc` — non-blank, non-comment lines in function body
- `parameterCount` — formal parameters (destructured objects count as 1)
- `maxNestingDepth` — maximum block nesting depth
- `returnCount` — number of `return` statements

**Normalized raw score:**

```text
SM_raw = 0.4×(loc/100) + 0.3×(params/6) + 0.2×(nesting/6) + 0.1×(returns/5)
```

---

## Composite Scoring Formula

### Saturation model

Based on the PMC paper's finding that perceived difficulty saturates — once code is complex enough, doubling raw complexity doesn't double how hard it feels:

```text
saturate(x) = 1 − e^(−k × x)    where k = 1.0
```

At x=1.0 (exactly at the F-tier threshold): `saturate ≈ 0.63`
At x=2.0 (twice F-tier): `saturate ≈ 0.86`
At x=3.0 (three times F-tier): `saturate ≈ 0.95`

This means once a function is in F-tier territory, further increases cause diminishing marginal penalty — reflecting actual developer experience.

### Final score

```text
penalty_i = saturate(raw_i) × weight_i × 100    for each pillar i
totalPenalty = Σ(penalty_i)
Quality Score = max(0, 100 − totalPenalty)
```

Higher score = better quality. Score 100 = no detected complexity.

### Grade bands

| Grade | Score | Meaning |
|-------|-------|---------|
| **A** | 80–100 | Clean, maintainable |
| **B** | 65–79 | Acceptable, minor improvements possible |
| **C** | 50–64 | Needs attention |
| **D** | 35–49 | Refactoring recommended |
| **F** | 0–34 | Critical complexity, refactor immediately |

`needsRefactoring = score < 65` (configurable via `refactoringThreshold`).

---

## Refactoring Flags

Each flag has two thresholds: **warn** (first trigger) and **error** (severe). Flags can be individually enabled, disabled, or customized via `qualitas.config.js`. `EXCESSIVE_RETURNS` is disabled by default.

| Flag | Warn | Error | Suggestion |
|------|------|-------|------------|
| `HIGH_COGNITIVE_FLOW` | CFC >= 13 | >= 19 | Extract nested branches into named functions |
| `HIGH_DATA_COMPLEXITY` | difficulty >= 26 | >= 41 | Reduce variable density; extract computations |
| `HIGH_IDENTIFIER_CHURN` | IRC >= 41 | >= 71 | Shorten function scope; break into smaller functions |
| `HIGH_HALSTEAD_EFFORT` | effort >= 1500 | >= 5000 | Simplify expressions; extract complex calculations |
| `TOO_MANY_PARAMS` | params >= 4 | >= 5 | Group related parameters into an options object |
| `TOO_LONG` | LOC >= 41 | >= 61 | Extract sub-functions to keep each under 40 lines |
| `DEEP_NESTING` | nesting >= 4 | >= 5 | Use early returns to flatten nesting |
| `HIGH_COUPLING` | imports >= 10 | >= 15 | Consider splitting into smaller modules |
| `EXCESSIVE_RETURNS` | returns >= 3 | >= 4 | Consolidate return paths (disabled by default) |

---

## CLI Reference

```text
qualitas <path> [options]

Arguments:
  path                          File or directory to analyze

Options:
  -f, --format <format>         Output format (default: text)
                                  text | compact | detail | flagged | json | markdown | summary
  -o, --output <path>           Output file for report formats (json, markdown)
  -c, --config <path>           Path to qualitas.config.js (overrides auto-detection)
  -p, --profile <name>          Weight profile: default | cc-focused | data-focused | strict
  -t, --threshold <n>           Exit code 1 if any score is below this (default: 65)
  --fail-on-flags <level>       Exit code 1 if any function has flags: warn | error
  --include-tests               Include test files (*.test.*, *.spec.*) in analysis
  -h, --help                    Show help
  -V, --version                 Show version
```

### Output formats

**Console formats** — print to stdout, exit code reflects threshold/flag violations:

| Format | Description |
|--------|-------------|
| `text` (default) | Per-function rows with flags |
| `detail` | Same as text with full metric breakdown per function |
| `flagged` | Only show functions that have flags |
| `compact` | One-line-per-file summary |
| `summary` | Executive summary with pillar health, grade histograms, and deduction breakdown |

**Report formats** — write to file (defaults to `qualitas-report.<ext>` or use `-o`), always exit 0:

| Format | Description |
|--------|-------------|
| `json` | Full report as JSON (for agents/pipelines) |
| `markdown` | Markdown tables with badge-style scores (for PRs) |

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | All scores at or above threshold (console formats), or report written successfully (json/markdown) |
| `1` | One or more scores below threshold, or flags detected at the configured severity (console formats only) |
| `2` | Parse error or file not found |

### `--fail-on-flags`

By default, the exit code is based only on the score threshold. With `--fail-on-flags`, the CLI also fails if any function has flags at the specified severity:

- `--fail-on-flags warn` — fail on any warning or error flag (zero tolerance)
- `--fail-on-flags error` — fail only on error-level flags

This can also be set in `qualitas.config.js` via the `failOnFlags` field.

### Examples

```bash
# Basic file analysis
qualitas ./src/payment.ts

# CI check — fail if any function scores below 70
qualitas ./src/ --threshold 70

# Zero-tolerance mode — fail on any warning or error flag
qualitas ./src/ --fail-on-flags warn

# Markdown report (writes to qualitas-report.md by default)
qualitas ./src/ -f markdown

# Markdown report to a specific path
qualitas ./src/ -f markdown -o quality-report.md

# JSON report to a specific path
qualitas ./src/ -f json -o report.json

# Use a config file from another location
qualitas /path/to/other-repo -c ./qualitas.config.js

# Executive summary with pillar health breakdown
qualitas ./src/ -f summary

# Use cc-focused profile (closer to pure SonarQube behavior)
qualitas ./src/ --profile cc-focused

# Show only flagged functions
qualitas ./src/ -f flagged
```

---

## Programmatic API

```typescript
import { quickScore, analyzeSource, analyzeFile, analyzeProject } from 'qualitas';
import type { FileQualityReport, QuickScore, AnalysisOptions, QualitasConfig } from 'qualitas';

// Fast check — returns only score, grade, and top flags (no full metric breakdown)
const qs: QuickScore = quickScore(`
  function add(a: number, b: number) { return a + b; }
`, 'add.ts');

console.log(qs.score);               // e.g. 98
console.log(qs.grade);               // 'A'
console.log(qs.needsRefactoring);    // false
console.log(qs.flaggedFunctionCount); // 0
console.log(qs.topFlags);            // []

// Full analysis — returns complete metric breakdown per function/class
const report = analyzeSource(`
  function add(a: number, b: number) {
    return a + b;
  }
`, 'add.ts');

console.log(report.score);   // e.g. 98
console.log(report.grade);   // 'A'
console.log(report.functions[0].metrics.cognitiveFlow.score); // raw CFC value

// Per-language flag overrides via config (e.g., stricter thresholds for Python)
const config: QualitasConfig = {
  languages: {
    python: { flags: { TOO_MANY_PARAMS: { warn: 3, error: 5 } } },
  },
};
const pyReport = analyzeSource(pythonSource, 'app.py', {}, config);

// Analyze a file (config parameter enables per-language flag resolution)
const fileReport = await analyzeFile('./src/payment.ts', {
  profile: 'strict',
  refactoringThreshold: 70,
});

// Analyze a full directory
const projectReport = await analyzeProject('./src/', {
  includeTests: false,
  refactoringThreshold: 65,
});

console.log(projectReport.summary.flaggedFunctions); // count of functions needing refactoring
```

### `quickScore` vs `analyzeSource`

| | `quickScore` | `analyzeSource` |
| --- | --- | --- |
| Return type | `QuickScore` | `FileQualityReport` |
| Per-function metrics | ✗ | ✓ |
| Score + grade | ✓ | ✓ |
| Top flags (up to 5) | ✓ | ✓ (per function) |
| Use case | Editor plugins, CI pass/fail | Full reports, dashboards |

### `QuickScore`

```typescript
interface QuickScore {
  score: number;               // 0–100 composite score
  grade: 'A' | 'B' | 'C' | 'D' | 'F';
  needsRefactoring: boolean;
  functionCount: number;
  flaggedFunctionCount: number;
  topFlags: RefactoringFlag[]; // up to 5, from the worst functions
}
```

### `AnalysisOptions`

```typescript
interface AnalysisOptions {
  // Named weight profile
  profile?: 'default' | 'cc-focused' | 'data-focused' | 'strict';

  // Override individual pillar weights (must sum to 1.0)
  weights?: {
    cognitiveFlow?: number;       // default: 0.30
    dataComplexity?: number;      // default: 0.25
    identifierReference?: number; // default: 0.20
    dependencyCoupling?: number;  // default: 0.15
    structural?: number;          // default: 0.10
  };

  // Score below which needsRefactoring = true (default: 65)
  refactoringThreshold?: number;

  // Include test files in project analysis (default: false)
  includeTests?: boolean;

  // File extensions to include (default: .ts .tsx .js .jsx .mjs .cjs .py .pyi)
  extensions?: string[];

  // Directories/files to exclude (default: .git, qualitas.config.js)
  // Configure additional excludes (node_modules, dist, etc.) via qualitas.config.js
  exclude?: string[];

  // Per-flag overrides — supports both camelCase and SCREAMING_SNAKE_CASE keys
  flagOverrides?: Record<string, FlagConfig>;
}
```

### `FileQualityReport`

```typescript
interface FileQualityReport {
  filePath: string;
  score: number;           // 0–100
  grade: 'A' | 'B' | 'C' | 'D' | 'F';
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
```

### `FunctionQualityReport`

```typescript
interface FunctionQualityReport {
  name: string;
  inferredName?: string;      // e.g. "const myFn = " for arrow functions
  score: number;
  grade: Grade;
  needsRefactoring: boolean;
  flags: RefactoringFlag[];
  metrics: MetricBreakdown;   // raw values for all 5 pillars
  scoreBreakdown: ScoreBreakdown; // per-pillar penalty contributions
  location: SourceLocation;
  isAsync: boolean;
  isGenerator: boolean;
}
```

---

## Weight Profiles

### `default` (recommended)

Weights derived from the PMC correlation coefficients, calibrated to avoid over-penalizing simple pattern matches (which inflate CFC but carry no real cognitive load):

| Pillar | Weight | Rationale |
| -------- | -------- | ----------- |
| DCI | 0.30 | Highest EEG correlation (r=0.901) — best indicator of real data complexity |
| IRC | 0.25 | Strongest single predictor (r=0.963) — captures variable-tracking cost |
| CFC | 0.20 | Good predictor but over-counts exhaustive matches and simple switches |
| DC | 0.15 | Important but less directly studied |
| SM | 0.10 | Useful sanity check, already partially captured by other pillars |

### `cc-focused`

Boosts CFC to 0.50. Behaves similarly to SonarQube Cognitive Complexity for teams transitioning from that tool.

### `data-focused`

Boosts DCI+IRC to 0.65 combined. Emphasizes Halstead/data complexity — useful for codebases with complex algorithms and data transformations.

### `strict`

Same weights as `default` but tighter grade bands: A≥90, B≥75, C≥60, D≥40.

---

## Sample Text Output

```text
qualitas: src/processPayment.ts
████░░░░░░  score: 42.0  grade: D

  ✗ processPayment()  [D]  score: 42  ← needs refactoring
    Flags:
    [error] Cognitive flow complexity is 44 (threshold: 19)
           → Extract nested branches into separate named functions. Use early returns to flatten the nesting hierarchy.
    [error] Identifier reference complexity is 178.2 (threshold: 71)
           → Variables are referenced many times across a wide scope. Break this function into smaller functions to shorten variable lifetimes.
    [error] Function has 6 parameters (threshold: 5)
           → Group related parameters into an options object: `{ option1, option2, ... }`.
    [error] Maximum nesting depth is 10 (threshold: 5)
           → Use early returns (guard clauses) to flatten the nesting hierarchy.

  ✓ validateCard()    [B]  score: 71
  ✓ formatAmount()    [A]  score: 97

File: D — 42.0  — 1 of 3 function(s) need refactoring
```

---

## Architecture

```text
qualitas/
├── crates/
│   ├── qualitas-core/          Language-agnostic analysis engine
│   │   ├── analyzer.rs         Orchestrator: extract → metrics → score → report
│   │   ├── types.rs            All Rust structs (serde → JSON → TypeScript)
│   │   ├── constants.rs        Thresholds, weights, saturation K, grade bands
│   │   ├── ir/                 Intermediate representation (event-based)
│   │   ├── languages/          Language adapters (TypeScript, Rust, Python)
│   │   ├── metrics/            5 metric collectors (CFC, DCI, IRC, DC, SM)
│   │   ├── scorer/             Composite score + flag generation
│   │   └── parser/             Shared parsing utilities (LOC counting, etc.)
│   │
│   ├── qualitas-cli/           Standalone Rust CLI binary
│   │   ├── main.rs             CLI entry point (clap)
│   │   ├── config.rs           qualitas.config.js loader
│   │   └── reporters/          text, compact, detail, flagged, json, markdown, summary
│   │
│   └── qualitas-napi/          Node.js native binding (napi-rs)
│
├── js/                         TypeScript wrapper (thin layer over native binding)
│   ├── index.ts                Public API: analyzeSource, analyzeFile, analyzeProject
│   ├── types.ts                TypeScript interfaces mirroring Rust serde structs
│   ├── cli.ts                  Commander CLI (Node.js)
│   └── reporters/              text, json, markdown reporters
│
├── tests/
│   ├── shared/                 Cross-cutting tests (scoring, config, reporters, project)
│   ├── typescript/             TypeScript adapter tests + fixtures
│   └── _template/              Template for adding new language adapter tests
│
├── qualitas_napi.js            Platform-aware native binding loader (auto-generated)
└── bin/qualitas.js             Node.js CLI entry point shim
```

### Supported languages

- **TypeScript / JavaScript** — via [oxc_parser](https://oxc.rs/) (`.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`)
- **Rust** — via [syn](https://docs.rs/syn) (`.rs`)
- **Python** — via [tree-sitter-python](https://crates.io/crates/tree-sitter-python) (`.py`, `.pyi`)

Adding a new language requires only one adapter file. See `CONTRIBUTING_LANGUAGE.md`.

### Why Rust + napi-rs?

The core analysis engine is written in Rust for three reasons:

1. **Speed:** `oxc_parser` (the parser powering Oxc/Biome/Rolldown) parses TypeScript at ~26ms vs SWC's ~84ms on large files. Arena-allocated AST with no GC pauses means analyzing 100+ file projects completes in under a second.

2. **Correctness:** Rust's type system enforces exhaustive handling of all AST node variants at compile time. Missing a node type is a compile error, not a runtime bug.

3. **Distribution:** The napi-rs optional-dependency pattern (same as SWC, Rspack, Oxlint) lets users `npm install` without any build toolchain. Pre-compiled binaries for all 5 platforms ship as separate optional packages and are selected automatically.

### Event-based IR

Language adapters parse source code and emit a stream of `QualitasEvent` values (control flow, operators, operands, nesting, etc.). The 5 metric collectors consume these events without knowing anything about the source language. This makes adding a new language a matter of writing one adapter file — the scoring engine is fully language-agnostic.

---

## Configuration

Create a `qualitas.config.js` in your project root. All fields are optional — CLI flags take priority.

Config file search order:

1. Explicit `-c` / `--config` path (if provided)
2. Walk up from the analyzed directory looking for `qualitas.config.js`
3. Look next to the `qualitas` executable

```javascript
module.exports = {
  // Exit code 1 if any function scores below this threshold (0-100)
  threshold: 80,

  // Fail on flags regardless of score: 'warn' | 'error'
  failOnFlags: 'error',

  // Weight profile: 'default' | 'cc-focused' | 'data-focused' | 'strict'
  profile: 'default',

  // Directories/files to exclude from analysis.
  // Only .git and qualitas.config.js are excluded by default — configure all others here.
  exclude: ['node_modules', 'dist', 'build', 'coverage', 'target'],

  // Per-flag configuration (enable/disable, custom thresholds).
  // Both camelCase and SCREAMING_SNAKE_CASE keys are supported.
  flags: {
    TOO_MANY_PARAMS: { warn: 5, error: 7 },
    EXCESSIVE_RETURNS: true,  // re-enable (disabled by default)
    DEEP_NESTING: false,      // disable entirely
  },

  // Per-language configuration
  languages: {
    typescript: {
      testPatterns: ['.test.', '.spec.', 'tests/'],
    },
    python: {
      testPatterns: ['test_', '_test.py', 'tests/', 'conftest.py'],
      // Per-language flag overrides (take precedence over global flags)
      flags: {
        TOO_MANY_PARAMS: { warn: 4, error: 6 },
      },
    },
    rust: {
      testPatterns: ['_test.rs', '_tests.rs', 'tests/'],
    },
  },
};
```

---

## Building from Source

**Prerequisites:** Rust toolchain (≥1.75), Node.js (≥18), `@napi-rs/cli` v3

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install napi-rs CLI
npm install -g @napi-rs/cli

# Clone and build
git clone https://github.com/An631/qualitas.git
cd qualitas
npm install

# Build native binary (debug, fast)
npm run build:debug

# Build native binary (release, optimized)
npm run build

# Build TypeScript wrapper
npm run build:ts

# Run all tests
cargo test -p qualitas-core   # 146 Rust unit tests
npm run test:ts               # 69 JS integration tests
```

---

## Testing

### Rust unit tests (`cargo test -p qualitas-core`)

146 tests across all modules. Cover:

- Per-language conformance tests (TypeScript, Rust, and Python adapters)
- CFC, DCI, IRC, DC, SM metric collectors via event-based IR
- Composite scorer saturation invariants (sublinearity, score bounds)
- Flag generation with default and custom thresholds
- Grade assignment across all profiles

### JavaScript integration tests (`npm run test:ts`)

69 tests across `tests/shared/` and `tests/typescript/`. Exercise the full stack (Rust → napi → JS):

- Scoring invariants, config loading/merging, reporter output, project analysis
- Flag overrides: camelCase and SCREAMING_SNAKE_CASE keys, custom thresholds, enable/disable
- Per-language flag overrides via config parameter
- TypeScript adapter: function collection patterns, class methods, arrow functions
- Python adapter: function/class extraction, comprehensions, decorators, async/await
- Weight profiles, threshold configuration, scope filtering

---

## License

MIT
