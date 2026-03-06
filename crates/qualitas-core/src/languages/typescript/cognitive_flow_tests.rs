use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::visit::walk;
use oxc_ast::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::analyzer::analyze_source_str;
use crate::metrics::cognitive_flow::compute_cfc;
use crate::types::{AnalysisOptions, CognitiveFlowResult};

// ─── AST-based CFC visitor (TypeScript-specific) ────────────────────────────

struct CfcVisitor {
    result: CognitiveFlowResult,
    nesting_depth: u32,
    fn_name: String,
}

impl CfcVisitor {
    fn new(fn_name: &str) -> Self {
        Self {
            result: CognitiveFlowResult {
                score: 0,
                nesting_penalty: 0,
                base_increments: 0,
                async_penalty: 0,
                max_nesting_depth: 0,
            },
            nesting_depth: 0,
            fn_name: fn_name.to_string(),
        }
    }

    fn add_nesting(&mut self) {
        self.result.score += 1 + self.nesting_depth;
        self.result.nesting_penalty += self.nesting_depth;
        self.result.base_increments += 1;
        if self.nesting_depth > self.result.max_nesting_depth {
            self.result.max_nesting_depth = self.nesting_depth;
        }
    }

    fn add_flat(&mut self) {
        self.result.score += 1;
        self.result.base_increments += 1;
    }

    fn add_async(&mut self) {
        let bonus = self.nesting_depth;
        self.result.score += 1 + bonus;
        self.result.async_penalty += 1 + bonus;
    }
}

impl<'a> Visit<'a> for CfcVisitor {
    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.add_nesting();
        self.visit_expression(&it.test);
        self.nesting_depth += 1;
        self.visit_statement(&it.consequent);

        if let Some(alt) = &it.alternate {
            match alt {
                Statement::IfStatement(_) => {
                    self.add_flat();
                    self.visit_statement(alt);
                }
                other => {
                    self.visit_statement(other);
                }
            }
        }

        self.nesting_depth -= 1;
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.add_nesting();
        self.nesting_depth += 1;
        walk::walk_for_statement(self, it);
        self.nesting_depth -= 1;
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.add_nesting();
        self.nesting_depth += 1;
        walk::walk_for_in_statement(self, it);
        self.nesting_depth -= 1;
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.add_nesting();
        self.nesting_depth += 1;
        walk::walk_for_of_statement(self, it);
        self.nesting_depth -= 1;
    }

    fn visit_while_statement(&mut self, it: &WhileStatement<'a>) {
        self.add_nesting();
        self.nesting_depth += 1;
        walk::walk_while_statement(self, it);
        self.nesting_depth -= 1;
    }

    fn visit_do_while_statement(&mut self, it: &DoWhileStatement<'a>) {
        self.add_nesting();
        self.nesting_depth += 1;
        walk::walk_do_while_statement(self, it);
        self.nesting_depth -= 1;
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.add_nesting();
        self.nesting_depth += 1;
        walk::walk_switch_statement(self, it);
        self.nesting_depth -= 1;
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.add_nesting();
        self.nesting_depth += 1;
        walk::walk_catch_clause(self, it);
        self.nesting_depth -= 1;
    }

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        self.add_flat();
        walk::walk_logical_expression(self, it);
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.add_flat();
        walk::walk_conditional_expression(self, it);
    }

    fn visit_labeled_statement(&mut self, it: &LabeledStatement<'a>) {
        self.add_flat();
        walk::walk_labeled_statement(self, it);
    }

    fn visit_break_statement(&mut self, it: &BreakStatement) {
        if it.label.is_some() {
            self.add_flat();
        }
    }

    fn visit_continue_statement(&mut self, it: &ContinueStatement) {
        if it.label.is_some() {
            self.add_flat();
        }
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if let Expression::Identifier(id) = &it.callee {
            if !self.fn_name.is_empty() && id.name.as_str() == self.fn_name {
                self.add_flat();
            }
        }

        if let Expression::StaticMemberExpression(member) = &it.callee {
            let prop = member.property.name.as_str();
            if prop == "then" || prop == "catch" {
                self.add_async();
            }
        }

        walk::walk_call_expression(self, it);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        if self.nesting_depth > 0 {
            self.result.score += self.nesting_depth;
            self.result.async_penalty += self.nesting_depth;
        }
        self.nesting_depth += 1;
        walk::walk_arrow_function_expression(self, it);
        self.nesting_depth -= 1;
    }

    fn visit_await_expression(&mut self, it: &AwaitExpression<'a>) {
        if self.nesting_depth > 1 {
            self.add_async();
        }
        walk::walk_await_expression(self, it);
    }
}

fn analyze_function_cfc_from_source(source: &str, fn_name: &str) -> CognitiveFlowResult {
    let alloc = Allocator::default();
    let st = SourceType::default()
        .with_typescript(true)
        .with_module(true);
    let result = Parser::new(&alloc, source, st).parse();
    for stmt in &result.program.body {
        if let Statement::FunctionDeclaration(f) = stmt {
            let mut visitor = CfcVisitor::new(fn_name);
            if let Some(body) = &f.body {
                visitor.visit_function_body(body);
            }
            return visitor.result;
        }
    }
    CfcVisitor::new(fn_name).result
}

// ── AST-based tests ─────────────────────────────────────────────────────

#[test]
fn empty_function_is_zero() {
    let r = analyze_function_cfc_from_source("function f() {}", "f");
    assert_eq!(r.score, 0);
}

#[test]
fn single_if_is_one() {
    let r = analyze_function_cfc_from_source("function f(x) { if (x) { return 1; } }", "f");
    assert_eq!(r.score, 1);
}

#[test]
fn nested_if_has_penalty() {
    let r = analyze_function_cfc_from_source(
        "function f(x, y) { if (x) { if (y) { return 1; } } }",
        "f",
    );
    assert_eq!(r.score, 3);
}

#[test]
fn logical_operator_adds_flat() {
    let r = analyze_function_cfc_from_source(
        "function f(a, b, c) { if (a && b || c) { return 1; } }",
        "f",
    );
    assert_eq!(r.score, 3);
}

// ── Semantic tests (adapter → event-based CFC) ─────────────────────────

#[test]
fn ts_simple_if_has_cfc_1() {
    let source = "function f(x: any) { if (x) {} }";
    let events = super::ts_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert_eq!(
        cfc.score, 1,
        "Simple if should have CFC score 1, got {}",
        cfc.score,
    );
}

#[test]
fn ts_nested_if_has_cfc_at_least_3() {
    let source = r"
function f(x: any, y: any) {
    if (x) {
        if (y) {
            return 1;
        }
    }
}
";
    let events = super::ts_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 3,
        "Nested if should have CFC score >= 3, got {}",
        cfc.score,
    );
}

#[test]
fn ts_function_with_5_params_flagged() {
    let source = "function f(a: number, b: number, c: number, d: number, e: number) { return a; }";
    let func = super::ts_first_fn(source);
    assert_eq!(
        func.param_count, 5,
        "Expected param_count=5, got {}",
        func.param_count,
    );
}

#[test]
fn ts_early_return_beats_if_else() {
    let early_return = r"
function grade(score: number): string {
    if (score >= 80) { return 'A'; }
    if (score >= 65) { return 'B'; }
    if (score >= 50) { return 'C'; }
    if (score >= 35) { return 'D'; }
    return 'F';
}
";
    let if_else = r"
function grade(score: number): string {
    if (score >= 80) {
        return 'A';
    } else if (score >= 65) {
        return 'B';
    } else if (score >= 50) {
        return 'C';
    } else if (score >= 35) {
        return 'D';
    } else {
        return 'F';
    }
}
";
    let opts = AnalysisOptions::default();
    let early_score = analyze_source_str(early_return, "early.ts", &opts)
        .unwrap()
        .functions[0]
        .score;
    let ifelse_score = analyze_source_str(if_else, "ifelse.ts", &opts)
        .unwrap()
        .functions[0]
        .score;

    assert!(
        early_score > ifelse_score,
        "Early return ({early_score:.1}) should score higher than if/else ({ifelse_score:.1})",
    );
}
