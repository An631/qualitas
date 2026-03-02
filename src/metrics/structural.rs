/// Structural Metrics (SM)
#[cfg(test)]
use oxc_ast::ast::*;
#[cfg(test)]
use oxc_ast::visit::walk;
#[cfg(test)]
use oxc_ast::Visit;
#[cfg(test)]
use oxc_syntax::scope::ScopeFlags;

use crate::parser::ast::count_loc;
use crate::types::StructuralResult;

#[cfg(test)]
struct SmVisitor {
    nesting_depth: u32,
    max_nesting_depth: u32,
    return_count: u32,
}

#[cfg(test)]
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

#[cfg(test)]
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
#[cfg(test)]
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

    let raw_score = compute_sm_raw(
        loc,
        param_count,
        visitor.max_nesting_depth,
        visitor.return_count,
    );

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

// ─── Event-based SM computation ─────────────────────────────────────────────

use crate::ir::events::QualitasEvent;

/// Compute structural metrics from a stream of IR events (language-agnostic).
///
/// `source`, `span_start`, `span_end` are used for LOC counting.
/// `param_count` comes from `FunctionExtraction.param_count`.
///
/// SM stops counting at `NestedFunctionEnter` boundaries — nested function
/// nesting and returns don't count toward the outer function's SM.
pub fn compute_sm_from_events(
    events: &[QualitasEvent],
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

    let mut nesting_depth: u32 = 0;
    let mut max_nesting_depth: u32 = 0;
    let mut return_count: u32 = 0;
    let mut nested_fn_depth: u32 = 0; // track nested function boundaries

    for event in events {
        match event {
            QualitasEvent::NestedFunctionEnter => {
                nested_fn_depth += 1;
            }
            QualitasEvent::NestedFunctionExit => {
                nested_fn_depth = nested_fn_depth.saturating_sub(1);
            }
            _ if nested_fn_depth > 0 => {
                // Skip events inside nested functions
            }
            QualitasEvent::NestingEnter => {
                nesting_depth += 1;
                if nesting_depth > max_nesting_depth {
                    max_nesting_depth = nesting_depth;
                }
            }
            QualitasEvent::NestingExit => {
                nesting_depth = nesting_depth.saturating_sub(1);
            }
            QualitasEvent::ReturnStatement => {
                return_count += 1;
            }
            _ => {}
        }
    }

    let raw_score = compute_sm_raw(loc, param_count, max_nesting_depth, return_count);

    StructuralResult {
        loc,
        total_lines,
        parameter_count: param_count,
        max_nesting_depth,
        return_count,
        method_count: None,
        raw_score,
    }
}

pub fn compute_sm_raw(loc: u32, params: u32, nesting: u32, returns: u32) -> f64 {
    use crate::constants::{
        NORM_SM_LOC, NORM_SM_NESTING, NORM_SM_PARAMS, NORM_SM_RETURNS, SM_LOC_WEIGHT,
        SM_NESTING_WEIGHT, SM_PARAMS_WEIGHT, SM_RETURNS_WEIGHT,
    };
    SM_LOC_WEIGHT * (f64::from(loc) / NORM_SM_LOC)
        + SM_PARAMS_WEIGHT * (f64::from(params) / NORM_SM_PARAMS)
        + SM_NESTING_WEIGHT * (f64::from(nesting) / NORM_SM_NESTING)
        + SM_RETURNS_WEIGHT * (f64::from(returns) / NORM_SM_RETURNS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

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
                    return analyze_structural_body(
                        body,
                        source,
                        f.span.start,
                        f.span.end,
                        param_count,
                    );
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
        let r =
            analyze_structural_from_source("function f(x) { if (x) { for (;;) { return 1; } } }");
        assert!(r.max_nesting_depth >= 2);
    }

    // ── Event-based tests ───────────────────────────────────────────────

    #[test]
    fn event_empty_function() {
        let source = "function f() {}";
        let events: Vec<QualitasEvent> = vec![];
        let r = compute_sm_from_events(&events, source, 0, source.len() as u32, 0);
        assert_eq!(r.parameter_count, 0);
        assert_eq!(r.return_count, 0);
        assert_eq!(r.max_nesting_depth, 0);
    }

    #[test]
    fn event_counts_returns_and_nesting() {
        let source = "function f(a) {\n  if (a) {\n    return 1;\n  }\n  return 0;\n}";
        let events = vec![
            QualitasEvent::NestingEnter,    // if block
            QualitasEvent::ReturnStatement, // return 1
            QualitasEvent::NestingExit,
            QualitasEvent::ReturnStatement, // return 0
        ];
        let r = compute_sm_from_events(&events, source, 0, source.len() as u32, 1);
        assert_eq!(r.parameter_count, 1);
        assert_eq!(r.return_count, 2);
        assert_eq!(r.max_nesting_depth, 1);
    }

    #[test]
    fn event_deep_nesting() {
        let source = "x";
        let events = vec![
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingExit,
            QualitasEvent::NestingExit,
            QualitasEvent::NestingExit,
        ];
        let r = compute_sm_from_events(&events, source, 0, source.len() as u32, 0);
        assert_eq!(r.max_nesting_depth, 3);
    }
}
