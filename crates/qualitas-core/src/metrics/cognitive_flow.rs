/// Cognitive Flow Complexity (CFC) — Enhanced CC-Sonar
///
/// Rules (all applied per function body):
/// - IfStatement, each for/while/do, switch, catch: +1 + nestingDepth (nesting penalty)
/// - else-if alternate IfStatement: +1 at same level (no extra nesting)
/// - LogicalExpression (&&, ||, ??): +1 flat per operator
/// - ConditionalExpression (ternary): +1 flat
/// - Recursive CallExpression (self-call by name): +1 flat
/// - LabeledStatement, labeled break/continue: +1 flat
/// - Promise .then/.catch call: +1 + nestingDepth (JS-specific)
/// - Nested ArrowFunctionExpression as callback arg: +nestingDepth (JS-specific)
/// - AwaitExpression inside nested scope (depth > 1): +1 + nestingDepth (JS-specific)
#[cfg(test)]
use oxc_ast::ast::*;
#[cfg(test)]
use oxc_ast::visit::walk;
#[cfg(test)]
use oxc_ast::Visit;

use crate::types::CognitiveFlowResult;

#[cfg(test)]
pub struct CfcVisitor {
    pub result: CognitiveFlowResult,
    nesting_depth: u32,
    fn_name: String,
}

#[cfg(test)]
impl CfcVisitor {
    pub fn new(fn_name: &str) -> Self {
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

#[cfg(test)]
impl<'a> Visit<'a> for CfcVisitor {
    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.add_nesting();
        // Visit the condition expression first — this is where &&/||/?? operators live
        self.visit_expression(&it.test);
        self.nesting_depth += 1;
        self.visit_statement(&it.consequent);

        if let Some(alt) = &it.alternate {
            match alt {
                Statement::IfStatement(_) => {
                    // else-if: +1 flat, no extra nesting push
                    self.add_flat();
                    self.visit_statement(alt);
                }
                other => {
                    // plain else: no increment
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
        // Recursive self-call detection
        if let Expression::Identifier(id) = &it.callee {
            if !self.fn_name.is_empty() && id.name.as_str() == self.fn_name {
                self.add_flat();
            }
        }

        // .then() / .catch() on a Promise chain → async complexity
        if let Expression::StaticMemberExpression(member) = &it.callee {
            let prop = member.property.name.as_str();
            if prop == "then" || prop == "catch" {
                self.add_async();
            }
        }

        walk::walk_call_expression(self, it);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        // A nested arrow function adds complexity proportional to current depth
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

/// Run CFC on a raw FunctionBody AST node.
#[cfg(test)]
pub fn analyze_cfc_body(body: &FunctionBody<'_>, fn_name: &str) -> CognitiveFlowResult {
    let mut visitor = CfcVisitor::new(fn_name);
    visitor.visit_function_body(body);
    visitor.result
}

// ─── Event-based CFC computation ────────────────────────────────────────────

use crate::ir::events::QualitasEvent;

/// Apply a control-flow increment: +1 base plus nesting depth penalty.
/// Returns `(score_delta, nesting_penalty_delta, base_increment_delta)`.
fn apply_control_flow_increment(nesting_depth: u32) -> (u32, u32, u32) {
    (1 + nesting_depth, nesting_depth, 1)
}

/// Apply an async-complexity increment: +1 plus nesting depth bonus.
/// Returns `(score_delta, async_penalty_delta)`.
fn apply_async_increment(nesting_depth: u32) -> (u32, u32) {
    let bonus = nesting_depth;
    (1 + bonus, 1 + bonus)
}

/// Mutable accumulator for CFC computation, replacing loose local variables.
struct CfcState {
    score: u32,
    nesting_depth: u32,
    nesting_penalty: u32,
    base_increments: u32,
    async_penalty: u32,
    max_nesting_depth: u32,
}

impl CfcState {
    fn new() -> Self {
        Self {
            score: 0,
            nesting_depth: 0,
            nesting_penalty: 0,
            base_increments: 0,
            async_penalty: 0,
            max_nesting_depth: 0,
        }
    }

    fn into_result(self) -> CognitiveFlowResult {
        CognitiveFlowResult {
            score: self.score,
            nesting_penalty: self.nesting_penalty,
            base_increments: self.base_increments,
            async_penalty: self.async_penalty,
            max_nesting_depth: self.max_nesting_depth,
        }
    }

    fn add_control_flow(&mut self) {
        let (sd, np, bi) = apply_control_flow_increment(self.nesting_depth);
        self.score += sd;
        self.nesting_penalty += np;
        self.base_increments += bi;
        self.track_max_nesting();
    }

    fn add_flat_increment(&mut self) {
        self.score += 1;
        self.base_increments += 1;
    }

    fn add_async_complexity(&mut self) {
        let (sd, ap) = apply_async_increment(self.nesting_depth);
        self.score += sd;
        self.async_penalty += ap;
    }

    fn add_nested_callback(&mut self) {
        if self.nesting_depth > 0 {
            self.score += self.nesting_depth;
            self.async_penalty += self.nesting_depth;
        }
    }

    fn enter_nesting(&mut self) {
        self.nesting_depth += 1;
        self.track_max_nesting();
    }

    fn exit_nesting(&mut self) {
        self.nesting_depth = self.nesting_depth.saturating_sub(1);
    }

    fn track_max_nesting(&mut self) {
        if self.nesting_depth > self.max_nesting_depth {
            self.max_nesting_depth = self.nesting_depth;
        }
    }
}

/// Handle a single IR event, updating the CFC accumulator.
fn process_cfc_event(event: &QualitasEvent, state: &mut CfcState) {
    match event {
        QualitasEvent::ControlFlow(_) => state.add_control_flow(),
        QualitasEvent::LogicOp(_) | QualitasEvent::RecursiveCall | QualitasEvent::LabeledFlow => {
            state.add_flat_increment();
        }
        QualitasEvent::AsyncComplexity(_) => state.add_async_complexity(),
        QualitasEvent::NestedCallback => state.add_nested_callback(),
        QualitasEvent::NestingEnter => state.enter_nesting(),
        QualitasEvent::NestingExit => state.exit_nesting(),
        _ => {}
    }
}

/// Compute CFC from a stream of IR events (language-agnostic).
///
/// Event ordering contract:
/// - `ControlFlow` is emitted BEFORE `NestingEnter` for the branch body,
///   so `nesting_depth` reflects the depth at the point of the branch.
/// - `NestingEnter`/`NestingExit` must be balanced.
pub fn compute_cfc(events: &[QualitasEvent]) -> CognitiveFlowResult {
    let mut state = CfcState::new();

    for event in events {
        process_cfc_event(event, &mut state);
    }

    state.into_result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::events::{AsyncEvent, ControlFlowEvent, ControlFlowKind, LogicOpEvent};
    use oxc_allocator::Allocator;
    use oxc_ast::Visit;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

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
        // outer if = +1+0=1, inner if = +1+1=2 → total 3
        let r = analyze_function_cfc_from_source(
            "function f(x, y) { if (x) { if (y) { return 1; } } }",
            "f",
        );
        assert_eq!(r.score, 3);
    }

    #[test]
    fn logical_operator_adds_flat() {
        // if=1, &&=1, ||=1 → 3
        let r = analyze_function_cfc_from_source(
            "function f(a, b, c) { if (a && b || c) { return 1; } }",
            "f",
        );
        assert_eq!(r.score, 3);
    }

    // ── Event-based tests ───────────────────────────────────────────────

    fn cf(kind: ControlFlowKind) -> QualitasEvent {
        QualitasEvent::ControlFlow(ControlFlowEvent {
            kind,
            has_else: false,
            else_is_if: false,
        })
    }

    #[test]
    fn event_empty_is_zero() {
        let r = compute_cfc(&[]);
        assert_eq!(r.score, 0);
        assert_eq!(r.max_nesting_depth, 0);
    }

    #[test]
    fn event_single_if_is_one() {
        let events = vec![
            cf(ControlFlowKind::If),
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingExit,
        ];
        let r = compute_cfc(&events);
        assert_eq!(r.score, 1); // +1+0 = 1
        assert_eq!(r.nesting_penalty, 0);
    }

    #[test]
    fn event_nested_if_has_penalty() {
        // outer if at depth=0: score += 1+0 = 1
        // nesting enters depth=1
        // inner if at depth=1: score += 1+1 = 2
        // total = 3
        let events = vec![
            cf(ControlFlowKind::If),
            QualitasEvent::NestingEnter,
            cf(ControlFlowKind::If),
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingExit,
            QualitasEvent::NestingExit,
        ];
        let r = compute_cfc(&events);
        assert_eq!(r.score, 3);
        assert_eq!(r.nesting_penalty, 1);
        assert_eq!(r.max_nesting_depth, 2);
    }

    #[test]
    fn event_logic_ops_are_flat() {
        // if(a && b || c) → if=1, &&=1, ||=1 → 3
        let events = vec![
            cf(ControlFlowKind::If),
            QualitasEvent::LogicOp(LogicOpEvent::And),
            QualitasEvent::LogicOp(LogicOpEvent::Or),
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingExit,
        ];
        let r = compute_cfc(&events);
        assert_eq!(r.score, 3);
        assert_eq!(r.base_increments, 3);
    }

    #[test]
    fn event_async_has_depth_penalty() {
        // promise chain at depth=1: score += 1+1 = 2, async_penalty = 2
        let events = vec![
            QualitasEvent::NestingEnter, // depth 1
            QualitasEvent::AsyncComplexity(AsyncEvent::PromiseChain),
            QualitasEvent::NestingExit,
        ];
        let r = compute_cfc(&events);
        assert_eq!(r.score, 2); // 1 + 1
        assert_eq!(r.async_penalty, 2);
    }

    #[test]
    fn event_nested_callback_at_depth() {
        // callback at depth 2 → score += 2, async_penalty += 2
        let events = vec![
            QualitasEvent::NestingEnter, // depth 1
            QualitasEvent::NestingEnter, // depth 2
            QualitasEvent::NestedCallback,
            QualitasEvent::NestingExit,
            QualitasEvent::NestingExit,
        ];
        let r = compute_cfc(&events);
        assert_eq!(r.score, 2);
        assert_eq!(r.async_penalty, 2);
    }

    #[test]
    fn event_recursive_call_is_flat() {
        let events = vec![QualitasEvent::RecursiveCall];
        let r = compute_cfc(&events);
        assert_eq!(r.score, 1);
        assert_eq!(r.base_increments, 1);
    }
}
