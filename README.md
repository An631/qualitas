# qualitas-ts

**TypeScript/JavaScript code quality measurement — Quality Score 0–100 based on cognitive science research.**

`qualitas-ts` measures code quality across five research-backed pillars and returns a single 0–100 **Quality Score** (higher = better). It is written in Rust using [oxc_parser](https://oxc.rs/) for native-speed analysis, distributed as a native npm package via [napi-rs](https://napi.rs/), and provides both a programmatic TypeScript API and a CLI.

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

`qualitas-ts` addresses all three gaps through a five-pillar composite score with an exponential saturation model.

> PMC paper: *"Measuring cognitive complexity in software development"* — https://pmc.ncbi.nlm.nih.gov/articles/PMC9942489/

---

## Quick start

```bash
# Analyze a file (text output, default)
npx qualitas ./src/myFile.ts

# Analyze a directory
npx qualitas ./src/

# JSON output for agents/automation
npx qualitas ./src/myFile.ts --format json

# Show only functions that need refactoring
npx qualitas ./src/ --flagged-only

# Fail CI if any score is below threshold
npx qualitas ./src/ --threshold 65
```

---

## Installation

```bash
npm install qualitas-ts
```

The correct native binary for your platform is installed automatically as an optional dependency:

| Platform | Package |
|----------|---------|
| macOS Apple Silicon | `@qualitas-ts/binding-darwin-arm64` |
| macOS Intel | `@qualitas-ts/binding-darwin-x64` |
| Linux x64 (glibc) | `@qualitas-ts/binding-linux-x64-gnu` |
| Linux arm64 (glibc) | `@qualitas-ts/binding-linux-arm64-gnu` |
| Windows x64 | `@qualitas-ts/binding-win32-x64-msvc` |

No build step required when installing from npm.

---

## The Five Pillars

### 1. Cognitive Flow Complexity (CFC) — 30% weight

An enhanced version of CC-Sonar tuned for TypeScript/JavaScript. Tracks nesting depth during AST traversal and applies progressively larger penalties for deeply nested control flow.

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

**Why better than raw CC-Sonar:** Adds JS-specific async/Promise chain penalties and models the real cost of deeply nested callbacks (a pattern common in JS/TS that CC-Sonar underweights).

---

### 2. Data Complexity Index (DCI) — 25% weight

Halstead-inspired metric. Addresses CC-Sonar's largest gap (r=0.901 correlation in the PMC paper vs CC-Sonar's r=0.513).

**Counting:**
- **Operators (η₁ distinct, N₁ total):** `+`, `-`, `*`, `/`, `===`, `!==`, `<`, `>`, `<=`, `>=`, `=`, `+=`, `-=`, `&&`, `||`, `??`, `!`, `typeof`, `instanceof`, `?.`, `??=`, etc.
- **Operands (η₂ distinct, N₂ total):** identifiers (variables/params), string/numeric/template literals, `this`, `null`, `undefined`, boolean literals

**Computed values:**
```
Vocabulary  η = η₁ + η₂
Length      N = N₁ + N₂
Volume      V = N × log₂(η)
Difficulty  D = (η₁/2) × (N₂/η₂)
Effort      E = D × V
```

**Normalized raw score:**
```
DCI_raw = 0.6 × (D / 60) + 0.4 × (V / 3000)
```
Where 60 = F-tier difficulty threshold, 3000 = F-tier volume threshold.

**Why it matters:** A function with 20 distinct variable names, 15 distinct operators, and 200 total token appearances forces the reader to hold a large mental vocabulary simultaneously. CC-Sonar scores this identically to a trivial function with the same branching structure.

---

### 3. Identifier Reference Complexity (IRC) — 20% weight

Novel metric. Inspired by the eye-tracking finding (r=0.963) that revisit count — how often a developer re-reads a variable while understanding code — is the strongest predictor of cognitive load.

**Algorithm:** For each declared identifier (variable, parameter, destructured binding) in function scope:
```
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
```
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
```
SM_raw = 0.4×(loc/100) + 0.3×(params/6) + 0.2×(nesting/6) + 0.1×(returns/5)
```

---

## Composite Scoring Formula

### Saturation model

Based on the PMC paper's finding that perceived difficulty saturates — once code is complex enough, doubling raw complexity doesn't double how hard it feels:

```
saturate(x) = 1 − e^(−k × x)    where k = 1.0
```

At x=1.0 (exactly at the F-tier threshold): `saturate ≈ 0.63`
At x=2.0 (twice F-tier): `saturate ≈ 0.86`
At x=3.0 (three times F-tier): `saturate ≈ 0.95`

This means once a function is in F-tier territory, further increases cause diminishing marginal penalty — reflecting actual developer experience.

### Final score

```
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

Generated when any metric exceeds the grade-C threshold:

| Flag | Trigger | Suggestion |
|------|---------|------------|
| `HIGH_COGNITIVE_FLOW` | CFC > 12 | "Extract nested branches into named functions" |
| `HIGH_DATA_COMPLEXITY` | DCI difficulty > 25 | "Reduce variable density; extract intermediate computations" |
| `HIGH_IDENTIFIER_CHURN` | IRC > 40 | "Shorten function scope; break into smaller functions" |
| `TOO_MANY_PARAMS` | params > 3 | "Group related parameters into an options object" |
| `TOO_LONG` | LOC > 40 | "Extract sub-functions to keep each under 40 lines" |
| `DEEP_NESTING` | nesting > 3 | "Use early returns to flatten nesting" |
| `HIGH_COUPLING` | importCount > 10 or distinctApiCalls > 8 | "Consider splitting into smaller modules" |
| `EXCESSIVE_RETURNS` | returns > 2 | "Consolidate return paths" |
| `HIGH_HALSTEAD_EFFORT` | effort > 1500 | "Simplify expressions; extract complex calculations" |

---

## CLI Reference

```
qualitas <path> [options]

Arguments:
  path                    File or directory to analyze

Options:
  -f, --format <format>   Output format: text | json | markdown  (default: text)
  -p, --profile <name>    Weight profile: default | cc-focused | data-focused | strict
  -t, --threshold <n>     Exit code 1 if any score is below this  (default: 65)
  --flagged-only          Only show items needing refactoring
  --verbose               Show full metric breakdown per function
  --scope <scope>         Report scope: function | class | file | module  (default: function)
  --include-tests         Include test files (*.test.ts, *.spec.ts)
  -h, --help              Show help
  -V, --version           Show version
```

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | All scores at or above threshold |
| `1` | One or more scores below threshold |
| `2` | Parse error or file not found |

### `--scope` detail

| Value | What is shown |
| --- | --- |
| `function` (default) | Per-function rows with flags and (optionally) metric breakdown |
| `class` | Class aggregate score and class-level flags only; standalone functions hidden |
| `file` | File header and score summary only; no function or class detail |
| `module` | Project summary stats only; no per-file expansion (project analysis only) |

### Examples

```bash
# Basic file analysis
qualitas ./src/payment.ts

# CI check — fail if any function scores below 70
qualitas ./src/ --threshold 70

# Markdown report (great for PRs)
qualitas ./src/ --format markdown > quality-report.md

# JSON for agent/pipeline consumption
qualitas ./src/ --format json | jq '.score'

# Use cc-focused profile (closer to pure SonarQube behavior)
qualitas ./src/ --profile cc-focused

# Show only flagged functions, verbose metrics
qualitas ./src/ --flagged-only --verbose
```

---

## Programmatic API

```typescript
import { quickScore, analyzeSource, analyzeFile, analyzeProject } from 'qualitas-ts';
import type { FileQualityReport, QuickScore, AnalysisOptions } from 'qualitas-ts';

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

// Analyze a file
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

  // Include *.test.ts / *.spec.ts files in project analysis (default: false)
  includeTests?: boolean;

  // File extensions to include (default: .ts .tsx .js .jsx .mjs .cjs)
  extensions?: string[];

  // Directory names to exclude (appended to defaults: node_modules, dist, build, .git)
  exclude?: string[];
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

Research-backed weights derived from the PMC correlation coefficients:

| Pillar | Weight | Rationale |
|--------|--------|-----------|
| CFC | 0.30 | Strong predictor, well-validated |
| DCI | 0.25 | Highest correlation (r=0.901) |
| IRC | 0.20 | Strongest single predictor (r=0.963) — lower weight because it's novel |
| DC | 0.15 | Important but less directly studied |
| SM | 0.10 | Useful sanity check, already partially captured by other pillars |

### `cc-focused`

Boosts CFC to 0.50. Behaves similarly to SonarQube Cognitive Complexity for teams transitioning from that tool.

### `data-focused`

Boosts DCI+IRC to 0.60 combined. Emphasizes Halstead/data complexity — useful for codebases with complex algorithms and data transformations.

### `strict`

Same weights as `default` but tighter grade bands: A≥90, B≥75, C≥60, D≥40.

---

## Sample Text Output

```
qualitas-ts: src/processPayment.ts
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

```
qualitas-ts/
├── src/                    Rust core (compiled to native .node binary)
│   ├── lib.rs              napi-rs exports: analyze_source(), quick_score()
│   ├── analyzer.rs         Orchestrator: parse → metrics → score → report
│   ├── types.rs            All Rust structs (serde → JSON → TypeScript)
│   ├── constants.rs        Thresholds, weights, saturation K, grade bands
│   ├── parser/
│   │   └── ast.rs          oxc_parser integration + function/class boundary extraction
│   ├── metrics/
│   │   ├── cognitive_flow.rs   CFC: enhanced CC-Sonar with async/Promise penalties
│   │   ├── data_complexity.rs  DCI: Halstead operators/operands counting
│   │   ├── identifier_refs.rs  IRC: scope-span × reference-count model
│   │   ├── dependencies.rs     DC: import analysis + API call detection
│   │   └── structural.rs       SM: LOC, params, nesting depth, returns
│   └── scorer/
│       ├── composite.rs    Saturation formula + weighted composite score
│       └── thresholds.rs   Grade assignment + refactoring flag generation
│
├── js/                     TypeScript wrapper (thin layer over native binding)
│   ├── index.ts            Public API: analyzeSource, analyzeFile, analyzeProject
│   ├── types.ts            TypeScript interfaces mirroring Rust serde structs
│   ├── cli.ts              Commander CLI
│   └── reporters/
│       ├── text.ts         Colored terminal output (picocolors)
│       ├── json.ts         JSON.stringify wrapper
│       └── markdown.ts     Markdown tables with badge-style scores
│
├── qualitas_ts.js          Platform-aware native binding loader (auto-generated)
├── qualitas_ts.d.ts        TypeScript definitions for the raw napi binding
│
├── tests/
│   ├── fixtures/           Sample .ts files at known quality levels
│   │   ├── clean.ts        Simple utilities → expected grade A
│   │   ├── deeply_nested.ts  6-level nesting → expected grade D/F
│   │   └── data_heavy.ts   Statistics functions → high DCI
│   └── js/
│       └── scorer.test.ts  Jest integration tests (exercises full native binding)
│
└── bin/
    └── qualitas.js         CLI entry point shim
```

### Why Rust + napi-rs?

The core analysis engine is written in Rust for three reasons:

1. **Speed:** `oxc_parser` (the parser powering Oxc/Biome/Rolldown) parses TypeScript at ~26ms vs SWC's ~84ms on large files. Arena-allocated AST with no GC pauses means analyzing 100+ file projects completes in under a second.

2. **Correctness:** Rust's type system enforces exhaustive handling of all AST node variants at compile time. Missing a node type is a compile error, not a runtime bug.

3. **Distribution:** The napi-rs optional-dependency pattern (same as SWC, Rspack, Oxlint) lets users `npm install` without any build toolchain. Pre-compiled binaries for all 5 platforms ship as separate optional packages and are selected automatically.

### AST traversal

Each metric module implements the `Visit<'a>` trait from `oxc_ast::visit`:

```rust
impl<'a> Visit<'a> for CfcVisitor {
    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.add_nesting();
        self.visit_expression(&it.test); // count &&/|| in condition
        self.nesting_depth += 1;
        self.visit_statement(&it.consequent);
        // handle else/else-if...
        self.nesting_depth -= 1;
    }
}
```

The analyzer runs all five visitors in a single AST pass per function, collecting metrics without re-parsing.

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
npx napi build --platform --js qualitas_ts.js --dts qualitas_ts.d.ts

# Build native binary (release, optimized)
npx napi build --platform --release --js qualitas_ts.js --dts qualitas_ts.d.ts

# Build TypeScript wrapper
npm run build:ts

# Run all tests
cargo test          # 17 Rust unit tests
npm test            # 6 JS integration tests
```

---

## Testing

### Rust unit tests (`cargo test`)

Located inline in each module (`#[cfg(test)]`). Cover:
- CFC increments for each control flow node type
- Halstead operator/operand counting
- IRC cost formula correctness
- DC import analysis
- SM LOC/nesting counting
- Composite scorer saturation invariants (sublinearity, score bounds)

### JavaScript integration tests (`npm test`)

35 tests in `tests/js/scorer.test.ts`. Exercise the full stack (Rust → napi binding → JS wrapper):

- Clean code scores ≥ 80, complex code triggers flags
- `TOO_MANY_PARAMS`, `DEEP_NESTING`, `TOO_LONG`, `HIGH_IDENTIFIER_CHURN` flags all verified
- `SourceLocation` reports 1-based line numbers (not byte offsets)
- DC metric correctly counts distinct API calls per function
- All function collection patterns: named functions, arrow functions, object literals, export default, class property arrows, class methods
- `quickScore()` returns compact summary matching `analyzeSource()` score
- `--scope` filtering (function/class/file) verified on reporter output
- Scoring invariants: monotonicity, bounds [0, 100], `ScoreBreakdown` penalty sum

---

## License

MIT
