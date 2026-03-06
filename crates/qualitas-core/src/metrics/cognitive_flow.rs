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
use crate::ir::events::ControlFlowKind;
use crate::types::CognitiveFlowResult;

// ─── Event-based CFC computation ────────────────────────────────────────────

use crate::ir::events::QualitasEvent;

/// Match/switch arms are flat branches of a single decision — much less
/// cognitive load than independent if/else chains. Discount their CFC
/// contribution so a 10-arm match costs ~2.5 CFC instead of 10.
const MATCH_ARM_DISCOUNT: f64 = 0.25;

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
    score: f64,
    nesting_depth: u32,
    nesting_penalty: u32,
    base_increments: u32,
    async_penalty: u32,
    max_nesting_depth: u32,
}

impl CfcState {
    fn new() -> Self {
        Self {
            score: 0.0,
            nesting_depth: 0,
            nesting_penalty: 0,
            base_increments: 0,
            async_penalty: 0,
            max_nesting_depth: 0,
        }
    }

    fn into_result(self) -> CognitiveFlowResult {
        CognitiveFlowResult {
            score: self.score.round() as u32,
            nesting_penalty: self.nesting_penalty,
            base_increments: self.base_increments,
            async_penalty: self.async_penalty,
            max_nesting_depth: self.max_nesting_depth,
        }
    }

    fn add_control_flow(&mut self) {
        let (sd, np, bi) = apply_control_flow_increment(self.nesting_depth);
        self.score += f64::from(sd);
        self.nesting_penalty += np;
        self.base_increments += bi;
        self.track_max_nesting();
    }

    /// Match/switch arms: discounted increment (flat branches of one decision).
    /// No nesting penalty — arms are parallel branches, not nested logic.
    fn add_match_arm(&mut self) {
        self.score += MATCH_ARM_DISCOUNT;
        self.base_increments += 1;
    }

    fn add_flat_increment(&mut self) {
        self.score += 1.0;
        self.base_increments += 1;
    }

    fn add_async_complexity(&mut self) {
        let (sd, ap) = apply_async_increment(self.nesting_depth);
        self.score += f64::from(sd);
        self.async_penalty += ap;
    }

    fn add_nested_callback(&mut self) {
        if self.nesting_depth > 0 {
            self.score += f64::from(self.nesting_depth);
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
    use QualitasEvent as E;
    match event {
        E::ControlFlow(cf) if is_match_arm(cf.kind) => state.add_match_arm(),
        E::ControlFlow(_) => state.add_control_flow(),
        E::LogicOp(_) | E::RecursiveCall | E::LabeledFlow => state.add_flat_increment(),
        E::AsyncComplexity(_) => state.add_async_complexity(),
        E::NestedCallback => state.add_nested_callback(),
        E::NestingEnter => state.enter_nesting(),
        E::NestingExit => state.exit_nesting(),
        _ => {}
    }
}

/// Match/switch arms get a discounted CFC increment.
fn is_match_arm(kind: ControlFlowKind) -> bool {
    matches!(kind, ControlFlowKind::ContextManager)
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
    fn event_match_arms_are_discounted() {
        // A match with 4 arms: PatternMatch + 4 × ContextManager
        // PatternMatch = +1 (full), each ContextManager = +0.25
        // Total: 1 + 4×0.25 = 2
        let events = vec![
            cf(ControlFlowKind::PatternMatch),
            QualitasEvent::NestingEnter,
            cf(ControlFlowKind::ContextManager), // arm 1
            cf(ControlFlowKind::ContextManager), // arm 2
            cf(ControlFlowKind::ContextManager), // arm 3
            cf(ControlFlowKind::ContextManager), // arm 4
            QualitasEvent::NestingExit,
        ];
        let r = compute_cfc(&events);
        assert_eq!(r.score, 2, "match(1) + 4 arms(4×0.25=1) = 2");
    }

    #[test]
    fn event_match_arms_no_nesting_penalty() {
        // Match arms inside a nesting block should NOT get nesting penalty.
        // The arms are flat branches of one decision.
        let events = vec![
            QualitasEvent::NestingEnter, // depth 1 (e.g., inside a function body)
            cf(ControlFlowKind::PatternMatch),
            QualitasEvent::NestingEnter,         // match body
            cf(ControlFlowKind::ContextManager), // arm at depth 2
            cf(ControlFlowKind::ContextManager), // arm at depth 2
            QualitasEvent::NestingExit,
            QualitasEvent::NestingExit,
        ];
        let r = compute_cfc(&events);
        // PatternMatch at depth 1: 1 + 1 = 2
        // Two arms: 2 × 0.25 = 0.5, rounds to 0
        // Total: 2 + 0.5 = 2.5, rounds to 3
        assert_eq!(r.score, 3, "match(2) + 2 arms(0.5) ≈ 3");
    }

    #[test]
    fn event_if_else_chain_costs_more_than_match() {
        // 4 if/else-if branches vs 4 match arms should have different CFC.
        // This validates that match arms are discounted.

        // if/else-if chain: each branch nests
        let if_else_events = vec![
            cf(ControlFlowKind::If),
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingExit,
            cf(ControlFlowKind::If), // else-if
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingExit,
            cf(ControlFlowKind::If), // else-if
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingExit,
            cf(ControlFlowKind::If), // else-if
            QualitasEvent::NestingEnter,
            QualitasEvent::NestingExit,
        ];
        let if_cfc = compute_cfc(&if_else_events).score;

        // match with 4 arms
        let match_events = vec![
            cf(ControlFlowKind::PatternMatch),
            QualitasEvent::NestingEnter,
            cf(ControlFlowKind::ContextManager),
            cf(ControlFlowKind::ContextManager),
            cf(ControlFlowKind::ContextManager),
            cf(ControlFlowKind::ContextManager),
            QualitasEvent::NestingExit,
        ];
        let match_cfc = compute_cfc(&match_events).score;

        assert!(
            match_cfc < if_cfc,
            "match({match_cfc}) should cost less CFC than if/else chain({if_cfc})",
        );
    }

    #[test]
    fn event_recursive_call_is_flat() {
        let events = vec![QualitasEvent::RecursiveCall];
        let r = compute_cfc(&events);
        assert_eq!(r.score, 1);
        assert_eq!(r.base_increments, 1);
    }
}
