/// Identifier Reference Complexity (IRC) — Novel metric
///
/// Inspired by eye-tracking revisit counts (r=0.963 with cognitive load).
///
/// For each declared identifier:
///   cost = reference_count × log2(scope_span_lines + 1)
/// Total IRC = Σ(cost)
use oxc_ast::ast::*;
use oxc_ast::visit::walk;
use oxc_ast::Visit;
use std::collections::HashMap;

use crate::types::{IdentifierHotspot, IdentifierRefResult};

#[derive(Debug, Default)]
struct IdentEntry {
    definition_line: u32,
    last_reference_line: u32,
    reference_count: u32,
}

struct IrcVisitor {
    entries: HashMap<String, IdentEntry>,
    source: String,
}

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

        hotspots.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));

        let total_irc: f64 = hotspots.iter().map(|h| h.cost).sum();
        hotspots.truncate(10);

        IdentifierRefResult { total_irc, hotspots }
    }
}

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

impl IrcVisitor {
    fn collect_pattern(&mut self, pattern: &BindingPattern<'_>) {
        match &pattern.kind {
            BindingPatternKind::BindingIdentifier(id) => {
                self.declare(id.name.as_str(), id.span.start);
            }
            BindingPatternKind::ObjectPattern(obj) => {
                for prop in &obj.properties {
                    self.collect_pattern(&prop.value);
                }
                if let Some(rest) = &obj.rest {
                    self.collect_pattern(&rest.argument);
                }
            }
            BindingPatternKind::ArrayPattern(arr) => {
                for elem in arr.elements.iter().flatten() {
                    self.collect_pattern(elem);
                }
                if let Some(rest) = &arr.rest {
                    self.collect_pattern(&rest.argument);
                }
            }
            BindingPatternKind::AssignmentPattern(assign) => {
                self.collect_pattern(&assign.left);
            }
        }
    }
}

/// Analyze IRC for a raw FunctionBody.
pub fn analyze_irc_body<'a>(body: &FunctionBody<'a>, source: &str) -> IdentifierRefResult {
    let mut visitor = IrcVisitor::new(source);
    visitor.visit_function_body(body);
    visitor.compute()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn analyze_irc_from_source(source: &str) -> IdentifierRefResult {
        let alloc = Allocator::default();
        let st = SourceType::default().with_typescript(true).with_module(true);
        let result = Parser::new(&alloc, source, st).parse();
        for stmt in &result.program.body {
            if let Statement::FunctionDeclaration(f) = stmt {
                if let Some(body) = &f.body {
                    return analyze_irc_body(body, source);
                }
            }
        }
        IdentifierRefResult { total_irc: 0.0, hotspots: vec![] }
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
}
