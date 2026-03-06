/// Structural Metrics (SM)
use crate::parser::ast::count_loc;
use crate::types::StructuralResult;

// ─── Event-based SM computation ─────────────────────────────────────────────

use crate::ir::events::QualitasEvent;

/// Mutable accumulator for structural event processing.
struct SmState {
    nesting_depth: u32,
    max_nesting_depth: u32,
    return_count: u32,
    nested_fn_depth: u32,
}

impl SmState {
    fn new() -> Self {
        Self {
            nesting_depth: 0,
            max_nesting_depth: 0,
            return_count: 0,
            nested_fn_depth: 0,
        }
    }

    /// Track nested function boundaries. Returns true if the event was consumed.
    fn handle_nested_fn_boundary(&mut self, event: &QualitasEvent) -> bool {
        match event {
            QualitasEvent::NestedFunctionEnter => {
                self.nested_fn_depth += 1;
                true
            }
            QualitasEvent::NestedFunctionExit => {
                self.nested_fn_depth = self.nested_fn_depth.saturating_sub(1);
                true
            }
            _ => false,
        }
    }

    fn is_inside_nested_fn(&self) -> bool {
        self.nested_fn_depth > 0
    }

    fn enter_nesting(&mut self) {
        self.nesting_depth += 1;
        if self.nesting_depth > self.max_nesting_depth {
            self.max_nesting_depth = self.nesting_depth;
        }
    }

    fn exit_nesting(&mut self) {
        self.nesting_depth = self.nesting_depth.saturating_sub(1);
    }

    /// Handle a single structural event that is NOT inside a nested function boundary.
    fn process_outer_event(&mut self, event: &QualitasEvent) {
        match event {
            QualitasEvent::NestingEnter => self.enter_nesting(),
            QualitasEvent::NestingExit => self.exit_nesting(),
            QualitasEvent::ReturnStatement => self.return_count += 1,
            _ => {}
        }
    }
}

/// Walk structural events and extract `(max_nesting, return_count, _)`.
///
/// Events inside `NestedFunctionEnter`/`NestedFunctionExit` boundaries are
/// skipped because nested functions are analyzed separately.
/// The third tuple element is reserved for future use and is always 0.
fn process_structural_events(events: &[QualitasEvent]) -> (u32, u32, u32) {
    let mut state = SmState::new();

    for event in events {
        if state.handle_nested_fn_boundary(event) {
            continue;
        }
        if state.is_inside_nested_fn() {
            continue;
        }
        state.process_outer_event(event);
    }

    (state.max_nesting_depth, state.return_count, 0)
}

/// Compute structural metrics from a stream of IR events (language-agnostic).
///
/// `source`, `span_start`, `span_end` are used for LOC counting.
/// `param_count` comes from `FunctionExtraction.param_count`.
///
/// SM stops counting at `NestedFunctionEnter` boundaries — nested function
/// nesting and returns don't count toward the outer function's SM.
/// Source span for LOC counting in structural metric computation.
pub struct SourceSpan<'a> {
    pub source: &'a str,
    pub start: u32,
    pub end: u32,
}

pub fn compute_sm_from_events(
    events: &[QualitasEvent],
    span: &SourceSpan<'_>,
    param_count: u32,
) -> StructuralResult {
    let loc = count_loc(span.source, span.start, span.end);
    let total_lines = span.source[span.start as usize..(span.end as usize).min(span.source.len())]
        .chars()
        .filter(|&c| c == '\n')
        .count() as u32
        + 1;

    let (max_nesting_depth, return_count, _) = process_structural_events(events);
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

/// Compute structural metrics from a stream of IR events with pre-computed LOC.
///
/// Used for file-scope analysis where the byte range is disjoint (multiple
/// non-contiguous statements), so LOC must be computed externally by summing
/// the LOC of each individual statement.
pub fn compute_sm_with_loc(
    events: &[QualitasEvent],
    loc: u32,
    total_lines: u32,
    param_count: u32,
) -> StructuralResult {
    let (max_nesting_depth, return_count, _) = process_structural_events(events);
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

    #[test]
    fn event_empty_function() {
        let source = "function f() {}";
        let events: Vec<QualitasEvent> = vec![];
        let span = SourceSpan {
            source,
            start: 0,
            end: source.len() as u32,
        };
        let r = compute_sm_from_events(&events, &span, 0);
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
        let span = SourceSpan {
            source,
            start: 0,
            end: source.len() as u32,
        };
        let r = compute_sm_from_events(&events, &span, 1);
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
        let span = SourceSpan {
            source,
            start: 0,
            end: source.len() as u32,
        };
        let r = compute_sm_from_events(&events, &span, 0);
        assert_eq!(r.max_nesting_depth, 3);
    }
}
