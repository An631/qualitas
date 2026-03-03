/// Identifier Reference Complexity (IRC) — Novel metric
///
/// Inspired by eye-tracking revisit counts (r=0.963 with cognitive load).
///
/// For each declared identifier:
///   cost = reference_count × log2(scope_span_lines + 1)
/// Total IRC = Σ(cost)
#[cfg(test)]
use oxc_ast::ast::*;
#[cfg(test)]
use oxc_ast::visit::walk;
#[cfg(test)]
use oxc_ast::Visit;
use std::collections::HashMap;

use crate::types::{IdentifierHotspot, IdentifierRefResult};

#[derive(Debug, Default)]
struct IdentEntry {
    definition_line: u32,
    last_reference_line: u32,
    reference_count: u32,
}

#[cfg(test)]
struct IrcVisitor {
    entries: HashMap<String, IdentEntry>,
    source: String,
}

#[cfg(test)]
impl IrcVisitor {
    fn new(source: &str) -> Self {
        Self {
            entries: HashMap::new(),
            source: source.to_string(),
        }
    }

    fn line(&self, offset: u32) -> u32 {
        crate::parser::ast::byte_to_line(&self.source, offset)
    }

    fn declare(&mut self, name: &str, offset: u32) {
        let line = self.line(offset);
        self.entries.entry(name.to_string()).or_insert(IdentEntry {
            definition_line: line,
            last_reference_line: line,
            reference_count: 0,
        });
    }

    fn reference(&mut self, name: &str, offset: u32) {
        let line = self.line(offset);
        if let Some(entry) = self.entries.get_mut(name) {
            entry.reference_count += 1;
            if line > entry.last_reference_line {
                entry.last_reference_line = line;
            }
        }
    }

    fn compute(self) -> IdentifierRefResult {
        let mut hotspots: Vec<IdentifierHotspot> = self
            .entries
            .into_iter()
            .filter(|(_, e)| e.reference_count > 0)
            .map(|(name, e)| {
                let span = e.last_reference_line.saturating_sub(e.definition_line);
                let cost = (e.reference_count as f64) * ((span as f64 + 1.0).log2());
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

        IdentifierRefResult {
            total_irc,
            hotspots,
        }
    }
}

#[cfg(test)]
impl<'a> Visit<'a> for IrcVisitor {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.declare(it.name.as_str(), it.span.start);
    }

    fn visit_formal_parameters(&mut self, it: &FormalParameters<'a>) {
        for param in &it.items {
            self.collect_pattern(&param.pattern);
        }
        walk::walk_formal_parameters(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.reference(it.name.as_str(), it.span.start);
    }
}

#[cfg(test)]
impl IrcVisitor {
    fn collect_pattern(&mut self, pattern: &BindingPattern<'_>) {
        match &pattern.kind {
            BindingPatternKind::BindingIdentifier(id) => {
                self.declare(id.name.as_str(), id.span.start);
            }
            BindingPatternKind::ObjectPattern(obj) => {
                self.collect_object_pattern(obj);
            }
            BindingPatternKind::ArrayPattern(arr) => {
                self.collect_array_pattern(arr);
            }
            BindingPatternKind::AssignmentPattern(assign) => {
                self.collect_pattern(&assign.left);
            }
        }
    }

    fn collect_object_pattern(&mut self, obj: &ObjectPattern<'_>) {
        for prop in &obj.properties {
            self.collect_pattern(&prop.value);
        }
        if let Some(rest) = &obj.rest {
            self.collect_pattern(&rest.argument);
        }
    }

    fn collect_array_pattern(&mut self, arr: &ArrayPattern<'_>) {
        for elem in arr.elements.iter().flatten() {
            self.collect_pattern(elem);
        }
        if let Some(rest) = &arr.rest {
            self.collect_pattern(&rest.argument);
        }
    }
}

/// Analyze IRC for a raw FunctionBody.
#[cfg(test)]
pub fn analyze_irc_body<'a>(body: &FunctionBody<'a>, source: &str) -> IdentifierRefResult {
    let mut visitor = IrcVisitor::new(source);
    visitor.visit_function_body(body);
    visitor.compute()
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
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn analyze_irc_from_source(source: &str) -> IdentifierRefResult {
        let alloc = Allocator::default();
        let st = SourceType::default()
            .with_typescript(true)
            .with_module(true);
        let result = Parser::new(&alloc, source, st).parse();
        for stmt in &result.program.body {
            if let Statement::FunctionDeclaration(f) = stmt {
                if let Some(body) = &f.body {
                    return analyze_irc_body(body, source);
                }
            }
        }
        IdentifierRefResult {
            total_irc: 0.0,
            hotspots: vec![],
        }
    }

    #[test]
    fn unused_variable_is_zero() {
        let r = analyze_irc_from_source("function f() { const x = 1; }");
        assert_eq!(r.total_irc, 0.0);
    }

    #[test]
    fn used_variable_has_cost() {
        let src = "function f() {\n  const x = 1;\n  return x + x;\n}";
        let r = analyze_irc_from_source(src);
        assert!(r.total_irc > 0.0);
    }

    // ── Event-based tests ───────────────────────────────────────────────

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
