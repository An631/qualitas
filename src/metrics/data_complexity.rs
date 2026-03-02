/// Data Complexity Index (DCI) — Halstead-inspired metric
///
/// Fills the gap CC-Sonar misses: variable/operator density.
/// From the PMC paper, Halstead Effort correlates r=0.901 with cognitive load.
#[cfg(test)]
use oxc_ast::ast::*;
#[cfg(test)]
use oxc_ast::visit::walk;
#[cfg(test)]
use oxc_ast::Visit;
use std::collections::HashSet;

use crate::types::{DataComplexityResult, HalsteadCounts};

#[cfg(test)]
pub struct DciVisitor {
    distinct_operators: HashSet<String>,
    distinct_operands: HashSet<String>,
    total_operators: u32,
    total_operands: u32,
}

#[cfg(test)]
impl DciVisitor {
    pub fn new() -> Self {
        Self {
            distinct_operators: HashSet::new(),
            distinct_operands: HashSet::new(),
            total_operators: 0,
            total_operands: 0,
        }
    }

    fn op(&mut self, name: &str) {
        self.distinct_operators.insert(name.to_string());
        self.total_operators += 1;
    }

    fn operand(&mut self, name: &str) {
        self.distinct_operands.insert(name.to_string());
        self.total_operands += 1;
    }

    pub fn compute(self) -> DataComplexityResult {
        let eta1 = self.distinct_operators.len() as f64;
        let eta2 = self.distinct_operands.len() as f64;
        let n1 = self.total_operators as f64;
        let n2 = self.total_operands as f64;

        let halstead = HalsteadCounts {
            distinct_operators: eta1 as u32,
            distinct_operands: eta2 as u32,
            total_operators: n1 as u32,
            total_operands: n2 as u32,
        };

        if eta1 + eta2 < 2.0 {
            return DataComplexityResult {
                halstead,
                difficulty: 0.0,
                volume: 0.0,
                effort: 0.0,
                raw_score: 0.0,
            };
        }

        let vocabulary = eta1 + eta2;
        let length = n1 + n2;
        let volume = if vocabulary > 1.0 {
            length * vocabulary.log2()
        } else {
            0.0
        };
        let difficulty = if eta2 > 0.0 {
            (eta1 / 2.0) * (n2 / eta2)
        } else {
            0.0
        };
        let effort = difficulty * volume;

        let raw_score =
            crate::constants::DCI_DIFFICULTY_WEIGHT * (difficulty / crate::constants::NORM_DCI_DIFFICULTY)
                + crate::constants::DCI_VOLUME_WEIGHT * (volume / crate::constants::NORM_DCI_VOLUME);

        DataComplexityResult {
            halstead,
            difficulty,
            volume,
            effort,
            raw_score,
        }
    }
}

#[cfg(test)]
impl<'a> Visit<'a> for DciVisitor {
    fn visit_binary_expression(&mut self, it: &BinaryExpression<'a>) {
        self.op(it.operator.as_str());
        walk::walk_binary_expression(self, it);
    }

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        self.op(it.operator.as_str());
        walk::walk_logical_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        self.op(it.operator.as_str());
        walk::walk_assignment_expression(self, it);
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        self.op(it.operator.as_str());
        walk::walk_unary_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        self.op(if it.prefix { "++pre" } else { "post++" });
        walk::walk_update_expression(self, it);
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.op("?:");
        walk::walk_conditional_expression(self, it);
    }

    fn visit_chain_expression(&mut self, it: &ChainExpression<'a>) {
        self.op("?.");
        walk::walk_chain_expression(self, it);
    }

    fn visit_ts_as_expression(&mut self, it: &TSAsExpression<'a>) {
        self.op("as");
        walk::walk_ts_as_expression(self, it);
    }

    fn visit_ts_type_assertion(&mut self, it: &TSTypeAssertion<'a>) {
        self.op("<type>");
        walk::walk_ts_type_assertion(self, it);
    }

    fn visit_ts_non_null_expression(&mut self, it: &TSNonNullExpression<'a>) {
        self.op("!");
        walk::walk_ts_non_null_expression(self, it);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.operand(it.name.as_str());
    }

    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        let key = &it.value.as_str()[..it.value.len().min(32)];
        self.operand(key);
    }

    fn visit_numeric_literal(&mut self, it: &NumericLiteral<'a>) {
        self.operand(&it.value.to_string());
    }

    fn visit_boolean_literal(&mut self, it: &BooleanLiteral) {
        self.operand(if it.value { "true" } else { "false" });
    }

    fn visit_null_literal(&mut self, _it: &NullLiteral) {
        self.operand("null");
    }

    fn visit_this_expression(&mut self, _it: &ThisExpression) {
        self.operand("this");
    }

    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        self.operand(&format!("tmpl#{}", it.span.start));
        walk::walk_template_literal(self, it);
    }
}

/// Analyze DCI for a raw FunctionBody.
#[cfg(test)]
pub fn analyze_dci_body<'a>(body: &FunctionBody<'a>) -> DataComplexityResult {
    let mut v = DciVisitor::new();
    v.visit_function_body(body);
    v.compute()
}

// ─── Event-based DCI computation ────────────────────────────────────────────

use crate::ir::events::*;

/// Compute DCI (Halstead metrics) from a stream of IR events (language-agnostic).
pub fn compute_dci(events: &[QualitasEvent]) -> DataComplexityResult {
    let mut distinct_operators: HashSet<String> = HashSet::new();
    let mut distinct_operands: HashSet<String> = HashSet::new();
    let mut total_operators: u32 = 0;
    let mut total_operands: u32 = 0;

    for event in events {
        match event {
            QualitasEvent::Operator(op) => {
                distinct_operators.insert(op.name.clone());
                total_operators += 1;
            }
            QualitasEvent::Operand(operand) => {
                distinct_operands.insert(operand.name.clone());
                total_operands += 1;
            }
            _ => {}
        }
    }

    let eta1 = distinct_operators.len() as f64;
    let eta2 = distinct_operands.len() as f64;
    let n1 = total_operators as f64;
    let n2 = total_operands as f64;

    let halstead = HalsteadCounts {
        distinct_operators: eta1 as u32,
        distinct_operands: eta2 as u32,
        total_operators: n1 as u32,
        total_operands: n2 as u32,
    };

    if eta1 + eta2 < 2.0 {
        return DataComplexityResult {
            halstead,
            difficulty: 0.0,
            volume: 0.0,
            effort: 0.0,
            raw_score: 0.0,
        };
    }

    let vocabulary = eta1 + eta2;
    let length = n1 + n2;
    let volume = if vocabulary > 1.0 {
        length * vocabulary.log2()
    } else {
        0.0
    };
    let difficulty = if eta2 > 0.0 {
        (eta1 / 2.0) * (n2 / eta2)
    } else {
        0.0
    };
    let effort = difficulty * volume;

    let raw_score =
        crate::constants::DCI_DIFFICULTY_WEIGHT * (difficulty / crate::constants::NORM_DCI_DIFFICULTY)
            + crate::constants::DCI_VOLUME_WEIGHT * (volume / crate::constants::NORM_DCI_VOLUME);

    DataComplexityResult {
        halstead,
        difficulty,
        volume,
        effort,
        raw_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_ast::Visit;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    fn analyze_dci_from_source(source: &str) -> DataComplexityResult {
        let alloc = Allocator::default();
        let st = SourceType::default().with_typescript(true).with_module(true);
        let result = Parser::new(&alloc, source, st).parse();
        for stmt in &result.program.body {
            if let Statement::FunctionDeclaration(f) = stmt {
                let mut v = DciVisitor::new();
                if let Some(body) = &f.body {
                    v.visit_function_body(body);
                }
                return v.compute();
            }
        }
        DciVisitor::new().compute()
    }

    #[test]
    fn empty_function_zeroes() {
        let r = analyze_dci_from_source("function f() {}");
        assert_eq!(r.difficulty, 0.0);
    }

    #[test]
    fn simple_addition_has_operators() {
        let r = analyze_dci_from_source("function f(a, b) { return a + b; }");
        assert!(r.halstead.total_operators >= 1);
        assert!(r.halstead.total_operands >= 2);
    }

    // ── Event-based tests ───────────────────────────────────────────────

    #[test]
    fn event_empty_is_zero() {
        let r = compute_dci(&[]);
        assert_eq!(r.difficulty, 0.0);
        assert_eq!(r.volume, 0.0);
        assert_eq!(r.effort, 0.0);
    }

    #[test]
    fn event_operators_and_operands() {
        let events = vec![
            QualitasEvent::Operand(OperandEvent { name: "a".into() }),
            QualitasEvent::Operator(OperatorEvent { name: "+".into() }),
            QualitasEvent::Operand(OperandEvent { name: "b".into() }),
        ];
        let r = compute_dci(&events);
        assert_eq!(r.halstead.distinct_operators, 1); // "+"
        assert_eq!(r.halstead.distinct_operands, 2);  // "a", "b"
        assert_eq!(r.halstead.total_operators, 1);
        assert_eq!(r.halstead.total_operands, 2);
        assert!(r.volume > 0.0);
        assert!(r.difficulty > 0.0);
    }

    #[test]
    fn event_repeated_operands_increase_difficulty() {
        let events = vec![
            QualitasEvent::Operand(OperandEvent { name: "x".into() }),
            QualitasEvent::Operator(OperatorEvent { name: "+".into() }),
            QualitasEvent::Operand(OperandEvent { name: "x".into() }),
            QualitasEvent::Operator(OperatorEvent { name: "+".into() }),
            QualitasEvent::Operand(OperandEvent { name: "x".into() }),
        ];
        let r = compute_dci(&events);
        // distinct_operators=1, distinct_operands=1, total_operators=2, total_operands=3
        assert_eq!(r.halstead.distinct_operators, 1);
        assert_eq!(r.halstead.distinct_operands, 1);
        assert_eq!(r.halstead.total_operators, 2);
        assert_eq!(r.halstead.total_operands, 3);
        // D = (η1/2) * (N2/η2) = (1/2) * (3/1) = 1.5
        assert!((r.difficulty - 1.5).abs() < 0.001);
    }
}
