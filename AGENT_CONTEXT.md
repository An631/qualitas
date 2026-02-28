# Agent Context — qualitas-ts

> **For LLM agents continuing this work.** This file contains the full technical context, architectural decisions, known issues, implementation details, and planned next steps. Read this before touching any code.

---

## Project Overview

`qualitas-ts` is a standalone npm package at `~/qualitas-ts/` (NOT inside the office-bohemia monorepo). It measures TypeScript/JavaScript code quality using a five-pillar composite Quality Score (0–100, higher = better). The core is written in Rust using `oxc_parser` and distributed via napi-rs as a native Node.js addon.

**Current state:** Fully functional. All 17 Rust unit tests pass. All 6 JS integration tests pass. CLI works end-to-end. One commit pushed to `https://github.com/An631/qualitas`.

---

## Repository & Environment

- **Local path:** `/home/vscode/qualitas-ts/`
- **GitHub:** `https://github.com/An631/qualitas` (private)
- **Runtime environment:** Linux x64 (Codespaces / Azure VM)
- **Rust toolchain:** stable, installed at `~/.cargo/` — always source with `. ~/.cargo/env` before running cargo/napi commands
- **Node.js:** v18+
- **napi-rs CLI:** `@napi-rs/cli` v3, installed globally at `~/.npm-global/`

**How to run commands:**
```bash
cd ~/qualitas-ts && . ~/.cargo/env && <command>
```

---

## Architecture: Rust Core + napi-rs + JS Wrapper

```
User code / CLI
      ↓
js/index.ts          (TypeScript — thin wrapper, no business logic)
      ↓
qualitas_ts.js       (Platform loader — picks correct .node binary)
      ↓
qualitas_ts.linux-x64-gnu.node   (Rust compiled via napi-rs)
      ↓
src/lib.rs           (napi-rs exports)
      ↓
src/analyzer.rs      (orchestrator)
      ↓
src/metrics/*.rs     (5 analysis modules using oxc AST visitor pattern)
      ↓
src/scorer/*.rs      (composite score + flags)
```

### Key design decisions

1. **Rust, not Node.js:** oxc_parser is 3× faster than SWC for large files. No GC pauses. Enables analyzing 100+ file projects in under a second.

2. **napi-rs optional dependency pattern:** Users `npm install qualitas-ts` without any Rust toolchain. Platform binaries are pre-compiled and ship as separate npm packages (`@qualitas-ts/binding-linux-x64-gnu`, etc.).

3. **JSON over napi objects:** The Rust functions return `String` (JSON-serialized), not napi objects. The JS wrapper does `JSON.parse()`. Reason: avoids napi object lifetime complexity, allows future WASM port with same interface.

4. **All metrics run per-function:** The analyzer collects all function bodies in one AST pass (`FnBodyCollector`), then runs all 5 metric visitors on each body independently. This avoids re-parsing.

5. **oxc_parser API:** Uses `oxc_allocator::Allocator` (arena allocator) + `oxc_parser::Parser`. The AST is tied to the allocator's lifetime. This means you CANNOT store `&Function<'a>` references across the allocator's lifetime — only store primitive data (byte offsets, counts, strings) from AST nodes.

---

## File-by-File Reference

### `Cargo.toml`
```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
napi = { version = "2", features = ["napi4", "serde-json"] }
napi-derive = "2"
oxc_parser = "0.46"
oxc_ast = "0.46"
oxc_allocator = "0.46"
oxc_span = "0.46"
oxc_syntax = "0.46"   # ← required for ScopeFlags
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[build-dependencies]
napi-build = "2"
```

**Important:** `oxc_syntax` must be included for `ScopeFlags`. It's used as a parameter in `visit_function`. `ScopeFlags` is at `oxc_syntax::scope::ScopeFlags`.

### `src/types.rs`
All public structs. Every struct derives `Serialize, Deserialize` with `#[serde(rename_all = "camelCase")]`. This is critical — the JS side reads camelCase JSON.

Key types:
- `FileQualityReport` — top-level output per file
- `FunctionQualityReport` — per-function result
- `ClassQualityReport` — per-class result (aggregates method scores)
- `MetricBreakdown` — holds all 5 raw metric results
- `ScoreBreakdown` — per-pillar penalty amounts
- `RefactoringFlag` — flag with type, severity, message, suggestion
- `AnalysisOptions` — input options (profile, weights, threshold)
- `WeightConfig` — per-pillar weights

### `src/constants.rs`
All magic numbers. Key values:
- `SATURATION_K = 1.0` — saturation rate. At x=1.0 (F-tier threshold), penalty is 63% of max. Was 0.15 originally (too gentle — complex code scored ~90). Changed to 1.0 after integration test failure.
- `NORM_CFC = 25.0` — F-tier CFC threshold (normalize CFC/25 to get raw score)
- `NORM_DCI_DIFFICULTY = 60.0` — F-tier Halstead difficulty
- `NORM_DCI_VOLUME = 3000.0` — F-tier Halstead volume
- `NORM_IRC = 100.0` — F-tier IRC total
- Grade bands: A≥80, B≥65, C≥50, D≥35, F<35
- `DEFAULT_REFACTORING_THRESHOLD = 65.0`

### `src/parser/ast.rs`
oxc_parser integration. **Key constraint:** You cannot store AST node references (`&Function<'a>`) across the allocator lifetime. The original implementation tried to use unsafe transmute to work around this — it was rewritten to only store metadata.

`BoundaryCollector` implements `Visit<'a>` and stores:
- `FunctionInfo { name, start_byte, end_byte, param_count, is_async, is_generator, inferred_name }`
- `ClassInfo { name, start_byte, end_byte }`

`parse_source(source, file_name) -> Result<ParsedFile, String>` — called once for dependency analysis (import records). The main analysis re-parses internally in `analyzer.rs` for the metric visitors.

`byte_to_line(source, offset) -> u32` — converts byte offset to 1-based line number.

### `src/metrics/cognitive_flow.rs`

`CfcVisitor` struct:
```rust
struct CfcVisitor<'a> {
    source: &'a str,
    fn_name: String,
    score: u32,
    nesting_penalty: u32,
    base_increments: u32,
    async_penalty: u32,
    nesting_depth: u32,
    max_nesting_depth: u32,
}
```

**Critical detail:** `visit_if_statement` MUST call `self.visit_expression(&it.test)` BEFORE incrementing nesting_depth. This is how `&&`/`||` operators in conditions get counted. If you skip this, logical operators in conditions are missed and CFC tests fail.

```rust
fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
    self.add_nesting();
    self.visit_expression(&it.test);  // ← CRITICAL: visit condition first
    self.nesting_depth += 1;
    self.visit_statement(&it.consequent);
    if let Some(alt) = &it.alternate {
        match alt {
            Statement::IfStatement(_) => { self.add_flat(); self.visit_statement(alt); }
            other => { self.visit_statement(other); }
        }
    }
    self.nesting_depth -= 1;
}
```

**Note on `alternate`:** `IfStatement.alternate` is `Option<Statement<'a>>`, NOT `Option<Box<Statement<'a>>>`. Use `match alt { ... }`, not `match alt.as_ref() { ... }`.

Arrow functions as callbacks: when a `CallExpression` has an `ArrowFunctionExpression` argument, the nesting_depth at that point is added as a penalty (models deeply nested callbacks).

`.then()`/`.catch()` detection: checks if a `CallExpression` is a member expression where the property is named "then" or "catch".

### `src/metrics/data_complexity.rs`

`DciVisitor` uses `HashSet<String>` for distinct operators/operands and `u32` counters for totals.

Operators detected: binary expression operators, assignment expression operators, unary operators (`!`, `typeof`, `void`, `delete`, `~`), logical operators, ternary, optional chaining (`?.`), nullish coalescing assignment.

Operands detected: identifiers, string literals, numeric literals, template literals (counted as one), boolean literals, `null`, `undefined`, `this`.

`compute()` method on `DciVisitor` returns `DataComplexityResult` with Halstead calculations. Edge case: if η=0 or η2=0, returns zeros (avoid division by zero).

```
V = N × log₂(η)                    // Volume
D = (η₁/2) × (N₂/η₂)              // Difficulty
E = D × V                           // Effort
raw_score = 0.6×(D/60) + 0.4×(V/3000)
```

### `src/metrics/identifier_refs.rs`

`IrcVisitor` tracks `HashMap<String, IdentEntry>`:
```rust
struct IdentEntry {
    definition_line: u32,
    last_reference_line: u32,
    reference_count: u32,
}
```

Declarations are tracked via `visit_variable_declarator` (captures let/const/var). Parameters are handled separately before the visitor runs (extracted from `Function.params`).

References are tracked via `visit_identifier_reference`.

Cost formula:
```rust
let span_lines = entry.last_reference_line.saturating_sub(entry.definition_line);
let cost = entry.reference_count as f64 * (span_lines as f64 + 1.0_f64).log2();
```

Hotspots: top-N entries sorted by cost, returned in `IdentifierRefResult.hotspots`.

### `src/metrics/dependencies.rs`

Two analysis functions:
- `analyze_file_dependencies(import_records)` — file-level import stats
- `analyze_function_dependencies(body, imported_names)` — detects `module.method()` call patterns

`DcFunctionVisitor` detects API calls by checking if a `CallExpression` is a member expression where the object is an identifier present in `imported_names` (the set of names imported at file level).

**Current limitation:** Only detects direct `importedName.method()` patterns. Doesn't detect:
- Destructured imports used directly: `import { readFile } from 'fs'` → `readFile()`
- Chained calls: `axios.get().then()`
- Re-exported or aliased imports

### `src/metrics/structural.rs`

`SmVisitor` tracks:
- Block nesting (push on enter, pop on exit)
- Return statement count
- Stops descent at nested function definitions (so inner function's LOC doesn't count toward outer function's LOC)

LOC = non-blank, non-comment lines. Counted by examining the source slice from `span.start` to `span.end`.

`compute_sm_raw(loc, params, nesting, returns) -> f64` — exported separately for class-level SM computation in analyzer.rs.

### `src/scorer/composite.rs`

```rust
pub fn saturate(x: f64) -> f64 {
    1.0 - (-SATURATION_K * x).exp()
}

pub fn compute_score(metrics, weights, profile) -> (f64, ScoreBreakdown) {
    let cfc_raw = metrics.cognitive_flow.score as f64 / NORM_CFC;
    let dci_raw = metrics.data_complexity.raw_score;  // already normalized 0-1+
    let irc_raw = metrics.identifier_reference.total_irc / NORM_IRC;
    let dc_raw = metrics.dependency_coupling.raw_score;
    let sm_raw = metrics.structural.raw_score;

    let cfc_penalty = saturate(cfc_raw) * 100.0 * weights.cognitive_flow;
    // ... etc
    let total = cfc_penalty + dci_penalty + irc_penalty + dc_penalty + sm_penalty;
    let score = (100.0 - total).max(0.0);
}
```

`aggregate_scores(reports: &[(f64, u32)]) -> f64` — LOC-weighted average for file/class scores.

### `src/scorer/thresholds.rs`

`grade_from_score(score, profile)` — uses `grade_bounds_for_profile` from constants.

`generate_flags(metrics)` — checks each metric against warning/error thresholds and generates `RefactoringFlag` entries. Flag types are string literals (not enum) to simplify napi serialization.

Flag severity: `"warning"` or `"error"`. A metric at grade-C boundary → warning. At grade-D boundary → error.

### `src/analyzer.rs`

`analyze_source_str(source, file_path, options) -> Result<FileQualityReport, String>`

**Two-parse pattern:**
1. First parse: uses `parse_source()` from `parser/ast.rs` to collect import records for file-level dependency analysis
2. Second parse: creates a fresh `Allocator` + `Parser` for the metric AST visitor pass

`FnBodyCollector` is the main AST visitor that collects functions and classes in one pass:
- `visit_function` — handles `function foo() {}` and `function() {}` (inside classes)
- `visit_variable_declarator` — handles `const foo = function() {}` and `const foo = () => {}`
- `visit_class` — pushes to class_stack, then walks; methods discovered inside go into the class

**Arrow function body access:**
```rust
// arrow.body is Box<'a, FunctionBody<'a>>
let body: &FunctionBody = &*arrow.body;  // deref the Box
```

### `src/lib.rs`

```rust
#[napi]
pub fn analyze_source(source: String, file_name: String, options_json: Option<String>) -> Result<String> {
    let options: AnalysisOptions = options_json
        .map(|j| serde_json::from_str(&j).unwrap_or_default())
        .unwrap_or_default();
    let report = analyzer::analyze_source_str(&source, &file_name, &options)
        .map_err(|e| napi::Error::from_reason(e))?;
    serde_json::to_string(&report).map_err(|e| napi::Error::from_reason(e.to_string()))
}

#[napi]
pub fn quick_score(source: String, file_name: String) -> Result<String> {
    // Returns compact { score, grade, needsRefactoring, flaggedCount } JSON
}
```

### `js/index.ts`

`getBinding()` tries in order:
1. `require('../qualitas_ts.js')` — the platform-aware loader (created manually since `napi build --js` didn't auto-generate it)
2. `@qualitas-ts/binding-${platform}-${arch}` — platform npm packages
3. Fallbacks with `-gnu` and `-msvc` suffixes
4. Throws if nothing found

The binding returns raw JSON strings. `analyzeSource()` calls `JSON.parse()` on the result and casts to `FileQualityReport`.

`analyzeFile()` — reads file, calls `analyzeSource()`, backfills `location.file` for each function/class (Rust doesn't know the full path during per-source analysis).

`analyzeProject()` — walks directory recursively, collects `.ts/.tsx/.js/.jsx/.mjs/.cjs` files, runs `analyzeFile()` on each in parallel via `Promise.all()`.

### `qualitas_ts.js`

**This file was created manually** because `napi build --js qualitas_ts.js` didn't generate it automatically in the dev environment. It's a platform-detection loader:

```js
switch (platform) {
  case 'linux':
    switch (arch) {
      case 'x64':
        if (isMusl()) { /* load musl */ } else { /* load gnu */ }
        break;
      case 'arm64': ...
    }
  case 'darwin': ...
  case 'win32': ...
}
module.exports.analyzeSource = nativeBinding.analyzeSource;
module.exports.quickScore = nativeBinding.quickScore;
```

If the local `.node` file exists, it loads it directly. Otherwise falls back to the npm package for that platform.

---

## oxc_parser API Patterns (oxc v0.46)

These are the correct patterns for oxc 0.46. The API changed significantly between versions.

### Visit trait

```rust
use oxc_ast::Visit;
use oxc_ast::visit::walk;
use oxc_syntax::scope::ScopeFlags;

impl<'a> Visit<'a> for MyVisitor {
    // function signature — note: &Function<'a> NOT &'a Function<'a>
    fn visit_function(&mut self, func: &Function<'a>, flags: ScopeFlags) {
        walk::walk_function(self, func, flags);  // recurse
    }

    // class — NO flags parameter (different from visit_function!)
    fn visit_class(&mut self, class: &Class<'a>) {
        walk::walk_class(self, class);
    }

    // if statement
    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        // it.alternate is Option<Statement<'a>>, not Option<Box<...>>
        if let Some(alt) = &it.alternate {
            match alt {
                Statement::IfStatement(inner) => { ... }
                _ => { ... }
            }
        }
    }

    // arrow function body
    // arrow.body is Box<'a, FunctionBody<'a>>
    let body: &FunctionBody = &*arrow.body;
}
```

### Critical import for ScopeFlags

```rust
use oxc_syntax::scope::ScopeFlags;
```

Without `oxc_syntax` in Cargo.toml, this won't compile.

### Parsing

```rust
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

let allocator = Allocator::default();
let source_type = SourceType::from_path(file_path)
    .unwrap_or_else(|_| SourceType::default().with_typescript(true));
let parse_result = Parser::new(&allocator, source, source_type).parse();
let program = &parse_result.program;

// Visitor usage
let mut visitor = MyVisitor::new();
visitor.visit_program(program);
```

---

## Bugs Fixed During Development

### 1. ScopeFlags import

**Error:** `ScopeFlags` not found in `oxc_ast::visit`
**Fix:** Import from `oxc_syntax::scope::ScopeFlags`; add `oxc_syntax = "0.46"` to Cargo.toml

### 2. Visit trait signature

**Error:** E0308 on every visitor method — `&'a IfStatement<'a>` doesn't match trait
**Fix:** Remove leading `'a` from all borrow parameters: `&IfStatement<'a>` not `&'a IfStatement<'a>`

### 3. visit_class parameter count

**Error:** E0050 — 3 params provided, trait expects 2
**Fix:** `visit_class` does NOT take `flags: ScopeFlags`. Only `visit_function` takes flags.

### 4. Unsafe transmute in parser/ast.rs

**Error:** Storing `&Function<'a>` across allocator lifetime causes UB
**Fix:** Rewrote to only store metadata (byte offsets, strings) not AST references

### 5. FunctionBody is a struct, not an enum

**Error:** E0599 trying to match `FunctionBody::FunctionBody(body)`
**Fix:** `let body: &FunctionBody = &*arrow.body` — direct Box deref

### 6. alternate is Option<Statement>, not Option<Box<Statement>>

**Error:** E0599 on `alt.as_ref()`
**Fix:** Use `match alt { ... }` directly without `.as_ref()`

### 7. CFC test failure: logical_operator_adds_flat

**Error:** Test expected CFC=3 for `if (a && b)`, got CFC=1
**Fix:** Added `self.visit_expression(&it.test)` before incrementing nesting_depth in `visit_if_statement`. Without this, `&&`/`||` in conditions were never visited.

### 8. napi build "Duplicate targets"

**Error:** `Internal Error: Duplicate targets are not allowed: aarch64-apple-darwin`
**Fix:** Updated package.json `napi` config from deprecated `triples` format to new `targets` array format with `binaryName`

### 9. qualitas_ts.js not generated

**Symptom:** `napi build --js qualitas_ts.js` didn't create the file
**Fix:** Created manually as a platform-detection loader

### 10. SATURATION_K = 0.15 too gentle

**Symptom:** Deeply nested function (CFC=44) scored 90.71 instead of < 65
**Root cause:** With k=0.15, at 1.76× the F-tier threshold, penalty was only 6.96/30 points
**Fix:** Changed to k=1.0. At same complexity level: penalty = 24.8/30 points. Test passes.

### 11. moduleResolution: "bundler" incompatible with module: "CommonJS"

**Error:** TS5095 — `bundler` requires `module: preserve` or `es2015+`
**Fix:** Changed to `moduleResolution: "node"` in tsconfig.json

---

## Tests

### Rust (`cargo test`) — 17 tests

Located in `#[cfg(test)]` blocks inside each module.

| File | Tests |
|------|-------|
| `src/metrics/cognitive_flow.rs` | 4 — simple_if_adds_one, nesting_increases_penalty, logical_operator_adds_flat, else_if_adds_flat |
| `src/metrics/data_complexity.rs` | 2 — empty_body, simple_addition_has_operators |
| `src/metrics/identifier_refs.rs` | 2 — unused_variable_is_zero, used_variable_has_cost |
| `src/metrics/dependencies.rs` | 2 — root_package_scoped, root_package_simple |
| `src/metrics/structural.rs` | 3 — empty_function, counts_params_and_returns, counts_nesting |
| `src/scorer/composite.rs` | 4 — perfect_code_scores_100, high_cfc_reduces_score, saturation_is_sublinear, aggregate_weighted_by_loc |

### JavaScript (`npm test`) — 6 tests

Located in `tests/js/scorer.test.ts`. All require native binding (loaded from `qualitas_ts.linux-x64-gnu.node`).

- `returns score >= 80 for trivial functions` — simple `add(a, b)`
- `returns no flags for simple utility` — `capitalize(s)` scores > 60
- `returns low score for deeply nested function` — processOrders with 7 nesting levels, scores < 65
- `flags too many params` — 6-param function triggers `TOO_MANY_PARAMS` error flag
- `clean code scores higher than complex code` — invariant test
- `score is always between 0 and 100` — tests extremes

---

## Known Limitations / Not Yet Implemented

### 1. Dependency Coupling at function level is zeroed out

In `analyzer.rs` `build_fn_report()`, the `DependencyCouplingResult` is hard-coded to all zeros:

```rust
let dc = DependencyCouplingResult {
    import_count: 0,
    distinct_sources: 0,
    // ...
    raw_score: 0.0,
};
```

The `analyze_function_dependencies()` function exists in `src/metrics/dependencies.rs` and works correctly, but it's not wired into the analyzer yet. **TODO:** Call `analyze_function_dependencies(&body, &imported_names)` instead of the zero struct.

### 2. `start_line` / `end_line` in SourceLocation are byte offsets, not line numbers

In `FunctionQualityReport.location`, `start_line` and `end_line` hold raw byte offsets from the AST (e.g., 42, 387), not 1-based line numbers. `byte_to_line()` in `parser/ast.rs` exists but isn't being called in `build_fn_report()`.

**TODO:** Call `byte_to_line(source, cf.start)` in the report builder to populate correct line numbers. Need to thread `source: &str` into `build_fn_report()`.

### 3. No WASM fallback

The plan included a WASM build for browser/edge environments. The `wasm/` directory was planned but not created.

**TODO:** Add `wasm-bindgen` features and create `wasm/` with a parallel build target. Update `js/index.ts` to try WASM if native binding fails.

### 4. npm/ platform packages not published

The `npm/` directory with per-platform `package.json` files was planned but not created. Currently, the package only works if you build from source locally.

**TODO:** Create `npm/darwin-arm64/package.json`, `npm/linux-x64-gnu/package.json`, etc. Set up GitHub Actions matrix build (the napi-rs CI template) to cross-compile all targets and publish.

### 5. No GitHub Actions CI

No `.github/workflows/` directory exists yet.

**TODO:** Create:
- `ci.yml` — run `cargo test` + `npm test` on push
- `publish.yml` — napi-rs matrix build + `npm publish` on tag

Use the napi-rs CI template from: https://napi.rs/docs/introduction/getting-started

### 6. Arrow functions in non-const declarations not analyzed

`FnBodyCollector` handles:
- `function foo() {}` (function declarations)
- `const foo = function() {}` (function expressions)
- `const foo = () => {}` (arrow in const declarator)
- Class methods (via `visit_class` + `visit_function`)

It does NOT currently handle:
- Arrow functions in object literals: `{ method: () => {} }`
- Arrow functions assigned to `let`/`var` (only handles `const`)
- Arrow functions as export default: `export default () => {}`
- Arrow functions in class property definitions: `class Foo { method = () => {} }`

**TODO:** Add `visit_object_expression` and `visit_property` to catch object method arrows. Add `visit_export_default_declaration`.

### 7. Class-level IRC and DCI not aggregated

`ClassQualityReport` computes `structural_metrics` by aggregating method LOC/nesting, but doesn't have aggregated CFC/DCI/IRC at the class level. The class score comes from averaging method scores.

**TODO:** Consider adding class-level metric aggregation (sum of method CFC, etc.).

### 8. `--scope` CLI option not implemented

The CLI accepts `--scope function | class | file | module` but the flag is parsed and ignored. All reports show function-level detail regardless.

**TODO:** Implement scope filtering in the reporters.

### 9. quickScore not exposed in JS API

`quick_score()` is exported from `src/lib.rs` via napi but never called from `js/index.ts`. It returns compact `{ score, grade, needsRefactoring, flaggedCount }` JSON.

**TODO:** Expose as `quickScore(source, fileName)` in the JS API for use cases where only the score is needed (e.g., editor plugins).

---

## Planned Next Steps (Priority Order)

### High priority

1. **Wire up function-level DC metric** — remove the zero-stub in `build_fn_report()`, call `analyze_function_dependencies()`. Also pass `imported_names` and the source correctly.

2. **Fix SourceLocation line numbers** — thread `source: &str` into `build_fn_report()` and call `byte_to_line()` for `start_line`/`end_line`.

3. **GitHub Actions CI** — add `.github/workflows/ci.yml` using napi-rs template. Ensures builds don't break on push.

4. **npm platform packages** — create the `npm/` subdirectory with per-platform `package.json` files so the package can be published correctly.

### Medium priority

5. **WASM fallback** — add `wasm-bindgen` target so the package works in browser/edge environments.

6. **Arrow functions in object literals** — extend `FnBodyCollector` to catch more function patterns.

7. **Expose `quickScore` in JS API** — useful for editor plugins and lightweight checks.

### Lower priority

8. **Implement `--scope` CLI filtering** — filter output to file/class/module granularity.

9. **Source maps in TS output** — the dist/ TypeScript output already generates source maps. Verify they work correctly.

10. **Benchmark harness** — add a `benches/` directory with criterion.rs benchmarks for the Rust core. Target: analyze 100-file project in < 500ms.

11. **VSCode extension** — this was discussed as a possible future integration. The `analyzeSource()` API is well-suited for this: call on document save, show inline diagnostics.

---

## How to Continue Development

### Setup (fresh session)

```bash
cd ~/qualitas-ts
. ~/.cargo/env          # activate Rust toolchain

# Verify state
cargo test              # should show 17 passed
npm test                # should show 6 passed

# Make sure the .node file is present (needed for npm test)
ls qualitas_ts.linux-x64-gnu.node
```

### Rebuild after Rust changes

```bash
cd ~/qualitas-ts && . ~/.cargo/env

# Fast debug build (for testing)
npx napi build --platform --js qualitas_ts.js --dts qualitas_ts.d.ts

# Optimized release build (for publishing)
npx napi build --platform --release --js qualitas_ts.js --dts qualitas_ts.d.ts

cargo test   # verify Rust tests
npm test     # verify JS integration
```

### Rebuild after TypeScript changes

```bash
cd ~/qualitas-ts
npm run build:ts     # compiles js/ → dist/
```

### Git workflow

```bash
cd ~/qualitas-ts
git add <specific files>
git commit -m "description"
git push origin main
```

The remote is already configured as `https://github.com/An631/qualitas.git`. You'll need to provide auth credentials (PAT via environment or credential store).

---

## Research Background

The PMC paper this tool is based on: https://pmc.ncbi.nlm.nih.gov/articles/PMC9942489/

Key findings:
1. **Halstead Effort** has the highest correlation (r=0.901) with measured cognitive load via EEG
2. **Eye-tracking revisit count** has the strongest correlation overall (r=0.963) — how often a developer re-reads a variable
3. **Saturation effect:** once code passes a complexity threshold, additional complexity doesn't proportionally increase difficulty. The brain hits a ceiling.
4. **Non-monotonicity:** raw CC-Sonar doesn't monotonically predict developer difficulty at all complexity levels

CC-Sonar drawbacks addressed by this tool:
- Ignores data complexity (variable density, Halstead effort) → DCI pillar
- Ignores identifier tracking cost (how hard it is to follow a variable) → IRC pillar
- Ignores the saturation effect → exponential saturation formula with k=1.0
- Ignores async/Promise complexity → CFC enhancements

---

## Contact / Ownership

Repository: https://github.com/An631/qualitas (private)
Built in: Microsoft Codespaces environment (office-bohemia codespace)
Session: Claude Code (claude-sonnet-4-6), February 2026
