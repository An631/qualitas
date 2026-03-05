/// Identifier Reference Complexity (IRC) — Novel metric
///
/// Inspired by eye-tracking revisit counts (r=0.963 with cognitive load).
///
/// For each declared identifier:
///   cost = reference_count × log2(scope_span_lines + 1)
/// Total IRC = Σ(cost)
use std::collections::HashMap;

use crate::types::{IdentifierHotspot, IdentifierRefResult};

#[derive(Debug, Default)]
struct IdentEntry {
    definition_line: u32,
    last_reference_line: u32,
    reference_count: u32,
}

// ─── Event-based IRC computation ────────────────────────────────────────────

use crate::ir::events::QualitasEvent;

/// Mutable accumulator for IRC computation, replacing loose local variables.
struct IrcState<'a> {
    entries: HashMap<String, IdentEntry>,
    nested_fn_depth: u32,
    source: &'a str,
}

impl<'a> IrcState<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            entries: HashMap::new(),
            nested_fn_depth: 0,
            source,
        }
    }

    fn into_result(self) -> IdentifierRefResult {
        let (hotspots, total_irc) = build_hotspots(self.entries);
        IdentifierRefResult {
            total_irc,
            hotspots,
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

    fn declare_ident(&mut self, name: &str, byte_offset: u32) {
        let line = crate::parser::ast::byte_to_line(self.source, byte_offset);
        self.entries.entry(name.to_string()).or_insert(IdentEntry {
            definition_line: line,
            last_reference_line: line,
            reference_count: 0,
        });
    }

    fn reference_ident(&mut self, name: &str, byte_offset: u32) {
        let line = crate::parser::ast::byte_to_line(self.source, byte_offset);
        if let Some(entry) = self.entries.get_mut(name) {
            entry.reference_count += 1;
            if line > entry.last_reference_line {
                entry.last_reference_line = line;
            }
        }
    }
}

/// Handle a single IR event, updating the IRC accumulator.
///
/// Returns early for events inside nested function boundaries.
fn process_irc_event(event: &QualitasEvent, state: &mut IrcState<'_>) {
    if state.handle_nested_fn_boundary(event) {
        return;
    }
    if state.is_inside_nested_fn() {
        return;
    }
    match event {
        QualitasEvent::IdentDeclaration(ident) => {
            state.declare_ident(&ident.name, ident.byte_offset);
        }
        QualitasEvent::IdentReference(ident) => {
            state.reference_ident(&ident.name, ident.byte_offset);
        }
        _ => {}
    }
}

/// Compute IRC from a stream of IR events (language-agnostic).
///
/// `source` is needed to convert byte offsets to line numbers for the scope-span calculation.
///
/// IRC ignores events inside `NestedFunctionEnter`/`NestedFunctionExit` boundaries,
/// since nested functions have their own identifier scopes.
pub fn compute_irc(events: &[QualitasEvent], source: &str) -> IdentifierRefResult {
    let mut state = IrcState::new(source);

    for event in events {
        process_irc_event(event, &mut state);
    }

    state.into_result()
}

/// Compute per-identifier costs, sort by descending cost, and return the top-10
/// hotspots along with the total IRC score.
fn build_hotspots(entries: HashMap<String, IdentEntry>) -> (Vec<IdentifierHotspot>, f64) {
    let mut hotspots: Vec<IdentifierHotspot> = entries
        .into_iter()
        .filter(|(_, e)| e.reference_count > 0)
        .map(|(name, e)| {
            let span = e.last_reference_line.saturating_sub(e.definition_line);
            let cost = f64::from(e.reference_count) * ((f64::from(span) + 1.0).log2());
            IdentifierHotspot {
                name,
                reference_count: e.reference_count,
                definition_line: e.definition_line,
                last_reference_line: e.last_reference_line,
                scope_span_lines: span,
                cost,
            }
        })
        .collect();

    hotspots.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let total_irc: f64 = hotspots.iter().map(|h| h.cost).sum();
    hotspots.truncate(10);

    (hotspots, total_irc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::events::IdentEvent;

    #[test]
    fn event_unused_is_zero() {
        // Declare x at offset 0, but never reference it
        let source = "const x = 1;\n";
        let events = vec![QualitasEvent::IdentDeclaration(IdentEvent {
            name: "x".into(),
            byte_offset: 6, // points somewhere in the source
        })];
        let r = compute_irc(&events, source);
        assert_eq!(r.total_irc, 0.0);
    }

    #[test]
    fn event_referenced_has_cost() {
        // source with 3 lines: decl on line 1, refs on line 3
        let source = "const x = 1;\nfoo();\nreturn x + x;\n";
        let events = vec![
            QualitasEvent::IdentDeclaration(IdentEvent {
                name: "x".into(),
                byte_offset: 6, // "x" in "const x = 1;"
            }),
            QualitasEvent::IdentReference(IdentEvent {
                name: "x".into(),
                byte_offset: 20, // "x" in "return x + x;"
            }),
            QualitasEvent::IdentReference(IdentEvent {
                name: "x".into(),
                byte_offset: 24, // second "x"
            }),
        ];
        let r = compute_irc(&events, source);
        assert!(r.total_irc > 0.0);
        assert_eq!(r.hotspots.len(), 1);
        assert_eq!(r.hotspots[0].name, "x");
        assert_eq!(r.hotspots[0].reference_count, 2);
    }
}
