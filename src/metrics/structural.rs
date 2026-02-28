/// Structural Metrics (SM)
use oxc_ast::ast::*;
use oxc_ast::visit::walk;
use oxc_ast::Visit;
use oxc_syntax::scope::ScopeFlags;

use crate::parser::ast::count_loc;
use crate::types::StructuralResult;

struct SmVisitor {
    nesting_depth: u32,
    max_nesting_depth: u32,
    return_count: u32,
}

impl SmVisitor {
    fn new() -> Self {
        Self { nesting_depth: 0, max_nesting_depth: 0, return_count: 0 }
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

    // Stop descent into nested functions — analyzed separately
    fn visit_function(&mut self, _it: &Function<'a>, _flags: ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _it: &ArrowFunctionExpression<'a>) {}
}

/// Analyze structural metrics for a function body.
pub fn analyze_structural_body(
    body: &FunctionBody<'_>,
    source: &str,
    span_start: u32,
    span_end: u32,
    param_count: u32,
) -> StructuralResult {
    let loc = count_loc(source, span_start, span_end);
    let total_lines = source[span_start as usize..(span_end as usize).min(source.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count() as u32
        + 1;

    let mut visitor = SmVisitor::new();
    visitor.visit_function_body(body);

    let raw_score = compute_sm_raw(loc, param_count, visitor.max_nesting_depth, visitor.return_count);

    StructuralResult {
        loc,
        total_lines,
        parameter_count: param_count,
        max_nesting_depth: visitor.max_nesting_depth,
        return_count: visitor.return_count,
        method_count: None,
        raw_score,
    }
}

pub fn compute_sm_raw(loc: u32, params: u32, nesting: u32, returns: u32) -> f64 {
    use crate::constants::*;
    SM_LOC_WEIGHT * (loc as f64 / NORM_SM_LOC)
        + SM_PARAMS_WEIGHT * (params as f64 / NORM_SM_PARAMS)
        + SM_NESTING_WEIGHT * (nesting as f64 / NORM_SM_NESTING)
        + SM_RETURNS_WEIGHT * (returns as f64 / NORM_SM_RETURNS)
}

/// Test helper.
pub fn analyze_structural_from_source(source: &str) -> StructuralResult {
    use oxc_allocator::Allocator;
    use oxc_ast::Visit;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    let alloc = Allocator::default();
    let st = SourceType::default().with_typescript(true).with_module(true);
    let result = Parser::new(&alloc, source, st).parse();
    for stmt in &result.program.body {
        if let Statement::FunctionDeclaration(f) = stmt {
            if let Some(body) = &f.body {
                let param_count = f.params.items.len() as u32;
                return analyze_structural_body(body, source, f.span.start, f.span.end, param_count);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
