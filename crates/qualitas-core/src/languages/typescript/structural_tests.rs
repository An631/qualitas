use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::visit::walk;
use oxc_ast::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::scope::ScopeFlags;

use crate::parser::ast::count_loc;
use crate::types::StructuralResult;

// ─── AST-based SM visitor (TypeScript-specific) ─────────────────────────────

struct SmVisitor {
    nesting_depth: u32,
    max_nesting_depth: u32,
    return_count: u32,
}

impl SmVisitor {
    fn new() -> Self {
        Self {
            nesting_depth: 0,
            max_nesting_depth: 0,
            return_count: 0,
        }
    }

    fn push(&mut self) {
        self.nesting_depth += 1;
        if self.nesting_depth > self.max_nesting_depth {
            self.max_nesting_depth = self.nesting_depth;
        }
    }

    fn pop(&mut self) {
        self.nesting_depth = self.nesting_depth.saturating_sub(1);
    }
}

impl<'a> Visit<'a> for SmVisitor {
    fn visit_block_statement(&mut self, it: &BlockStatement<'a>) {
        self.push();
        walk::walk_block_statement(self, it);
        self.pop();
    }

    fn visit_return_statement(&mut self, it: &ReturnStatement<'a>) {
        self.return_count += 1;
        walk::walk_return_statement(self, it);
    }

    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {}
}

fn analyze_structural_from_source(source: &str) -> StructuralResult {
    let alloc = Allocator::default();
    let st = SourceType::default()
        .with_typescript(true)
        .with_module(true);
    let result = Parser::new(&alloc, source, st).parse();
    for stmt in &result.program.body {
        if let Statement::FunctionDeclaration(f) = stmt {
            if let Some(body) = &f.body {
                let param_count = f.params.items.len() as u32;
                let loc = count_loc(source, f.span.start, f.span.end);
                let total_lines = source
                    [f.span.start as usize..(f.span.end as usize).min(source.len())]
                    .chars()
                    .filter(|&c| c == '\n')
                    .count() as u32
                    + 1;

                let mut visitor = SmVisitor::new();
                visitor.visit_function_body(body);

                let raw_score = crate::metrics::structural::compute_sm_raw(
                    loc,
                    param_count,
                    visitor.max_nesting_depth,
                    visitor.return_count,
                );

                return StructuralResult {
                    loc,
                    total_lines,
                    parameter_count: param_count,
                    max_nesting_depth: visitor.max_nesting_depth,
                    return_count: visitor.return_count,
                    method_count: None,
                    raw_score,
                };
            }
        }
    }
    StructuralResult {
        loc: 0,
        total_lines: 0,
        parameter_count: 0,
        max_nesting_depth: 0,
        return_count: 0,
        method_count: None,
        raw_score: 0.0,
    }
}

#[test]
fn empty_function() {
    let r = analyze_structural_from_source("function f() {}");
    assert_eq!(r.parameter_count, 0);
    assert_eq!(r.return_count, 0);
}

#[test]
fn counts_params_and_returns() {
    let r = analyze_structural_from_source("function f(a, b, c) { return a + b + c; }");
    assert_eq!(r.parameter_count, 3);
    assert_eq!(r.return_count, 1);
}

#[test]
fn counts_nesting() {
    let r = analyze_structural_from_source("function f(x) { if (x) { for (;;) { return 1; } } }");
    assert!(r.max_nesting_depth >= 2);
}

// ── Logical LOC (statement_count) tests ─────────────────────────────────────

fn ts_first_fn_statement_count(source: &str) -> Option<u32> {
    use crate::ir::language::LanguageAdapter;
    let adapter = super::TypeScriptAdapter;
    let extraction = adapter.extract(source, "test.ts").unwrap();
    extraction
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function")
        .statement_count
}

fn ts_first_fn_report(source: &str) -> crate::types::FunctionQualityReport {
    let report = crate::analyzer::analyze_source_str(
        source,
        "test.ts",
        &crate::types::AnalysisOptions::default(),
    )
    .unwrap();
    report
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function in TS report")
}

#[test]
fn ts_statement_count_single_return() {
    let source = "function add(a: number, b: number): number { return a + b; }";
    assert_eq!(
        ts_first_fn_statement_count(source),
        Some(1),
        "Single return should give statement_count = 1",
    );
}

#[test]
fn ts_statement_count_empty_function() {
    let source = "function empty() {}";
    assert_eq!(ts_first_fn_statement_count(source), Some(0));
}

#[test]
fn ts_statement_count_includes_nested_block_statements() {
    let source = r"
function check(x: number): number {
    if (x > 0) {
        const y = x * 2;
        return y;
    }
    return x;
}
";
    // 4 statements: if (+ nested const y, return y), return x
    assert_eq!(
        ts_first_fn_statement_count(source),
        Some(4),
        "Expected 4 statements (including nested)",
    );
}

#[test]
fn ts_statement_count_for_loop_body() {
    let source = r"
function process(items: number[]): number {
    let total = 0;
    for (const item of items) {
        if (item > 0) {
            total += item;
        }
    }
    return total;
}
";
    // 5 statements: let total, for (+ if (+ total +=)), return total
    assert_eq!(
        ts_first_fn_statement_count(source),
        Some(5),
        "Expected 5 statements (top-level + nested)",
    );
}

#[test]
fn ts_statement_count_switch_cases() {
    let source = r#"
function describe(x: number): string {
    switch (x) {
        case 1:
            return "one";
        case 2:
            return "two";
        default:
            return "other";
    }
}
"#;
    // 4 statements: switch + 3 case returns
    assert_eq!(
        ts_first_fn_statement_count(source),
        Some(4),
        "Expected 4 statements (switch + 3 case returns)",
    );
}

#[test]
fn ts_statement_count_try_catch() {
    let source = r"
function safeParse(text: string): number {
    try {
        return parseInt(text);
    } catch (e) {
        return 0;
    }
}
";
    // 3 statements: try (+ return parseInt), catch (+ return 0)
    assert_eq!(
        ts_first_fn_statement_count(source),
        Some(3),
        "Expected 3 statements (try/catch + nested returns)",
    );
}

#[test]
fn ts_logical_loc_used_in_structural_metric() {
    let source = r"
function add(a: number, b: number): number {
    return a + b;
}
";
    let report = ts_first_fn_report(source);
    assert_eq!(
        report.metrics.structural.loc, 1,
        "Structural metric should use logical LOC (1 statement), got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn ts_logical_loc_includes_nested_statements() {
    let source = r"
function check(x: number): number {
    if (x > 0) {
        const y = x * 2;
        return y;
    }
    return x;
}
";
    let report = ts_first_fn_report(source);
    assert_eq!(
        report.metrics.structural.loc, 4,
        "Logical LOC should include nested statements, got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn ts_physical_loc_in_total_lines() {
    let source = r"
function add(a: number, b: number): number {
    return a + b;
}
";
    let report = ts_first_fn_report(source);
    assert!(
        report.metrics.structural.total_lines >= 3,
        "total_lines should be >= 3 physical lines, got {}",
        report.metrics.structural.total_lines,
    );
}

#[test]
fn ts_file_score_is_loc_weighted() {
    let source = r"
function tiny(a: number): number { return a; }

function longer(x: number): number {
    let result = 0;
    if (x > 0) {
        if (x > 10) {
            if (x > 100) { result = x * 2; } else { result = x + 1; }
        } else { result = x - 1; }
    } else { result = -x; }
    return result;
}
";
    let report = crate::analyzer::analyze_source_str(
        source,
        "weighted.ts",
        &crate::types::AnalysisOptions::default(),
    )
    .unwrap();
    let tiny_score = report.functions[0].score;
    let longer_score = report.functions[1].score;
    assert!(
        longer_score < tiny_score,
        "Longer function ({longer_score:.1}) should score lower than tiny ({tiny_score:.1})",
    );
}
