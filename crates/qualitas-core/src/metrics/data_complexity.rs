/// Data Complexity Index (DCI) — Halstead-inspired metric
///
/// Fills the gap CC-Sonar misses: variable/operator density.
/// From the PMC paper, Halstead Effort correlates r=0.901 with cognitive load.
use std::collections::HashSet;

use crate::types::{DataComplexityResult, HalsteadCounts};

// ─── Event-based DCI computation ────────────────────────────────────────────

use crate::ir::events::QualitasEvent;

/// Compute Halstead volume, difficulty, and effort from the four base counts.
///
/// Returns `(volume, difficulty, effort)`. If the vocabulary is too small
/// (`eta1 + eta2 < 2`), all three values are zero.
fn compute_halstead_metrics(eta1: f64, eta2: f64, n1: f64, n2: f64) -> (f64, f64, f64) {
    if eta1 + eta2 < 2.0 {
        return (0.0, 0.0, 0.0);
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

    (volume, difficulty, effort)
}

/// Compute the weighted raw DCI score from difficulty and volume.
fn compute_halstead_score(difficulty: f64, volume: f64) -> f64 {
    crate::constants::DCI_DIFFICULTY_WEIGHT * (difficulty / crate::constants::NORM_DCI_DIFFICULTY)
        + crate::constants::DCI_VOLUME_WEIGHT * (volume / crate::constants::NORM_DCI_VOLUME)
}

/// Mutable accumulator for Halstead operator/operand counting.
struct HalsteadAccumulator {
    distinct_operators: HashSet<String>,
    distinct_operands: HashSet<String>,
    total_operators: u32,
    total_operands: u32,
}

impl HalsteadAccumulator {
    fn new() -> Self {
        Self {
            distinct_operators: HashSet::new(),
            distinct_operands: HashSet::new(),
            total_operators: 0,
            total_operands: 0,
        }
    }

    fn accumulate(&mut self, event: &QualitasEvent) {
        match event {
            QualitasEvent::Operator(op) => {
                self.distinct_operators.insert(op.name.clone());
                self.total_operators += 1;
            }
            QualitasEvent::Operand(operand) => {
                self.distinct_operands.insert(operand.name.clone());
                self.total_operands += 1;
            }
            _ => {}
        }
    }

    fn into_counts(self) -> HalsteadCounts {
        HalsteadCounts {
            distinct_operators: self.distinct_operators.len() as u32,
            distinct_operands: self.distinct_operands.len() as u32,
            total_operators: self.total_operators,
            total_operands: self.total_operands,
        }
    }
}

/// Collect Halstead operator/operand counts from IR events.
fn collect_halstead_counts(events: &[QualitasEvent]) -> HalsteadCounts {
    let mut acc = HalsteadAccumulator::new();
    for event in events {
        acc.accumulate(event);
    }
    acc.into_counts()
}

/// Compute DCI (Halstead metrics) from a stream of IR events (language-agnostic).
pub fn compute_dci(events: &[QualitasEvent]) -> DataComplexityResult {
    let halstead = collect_halstead_counts(events);

    let eta1 = f64::from(halstead.distinct_operators);
    let eta2 = f64::from(halstead.distinct_operands);
    let n1 = f64::from(halstead.total_operators);
    let n2 = f64::from(halstead.total_operands);

    let (volume, difficulty, effort) = compute_halstead_metrics(eta1, eta2, n1, n2);
    let raw_score = compute_halstead_score(difficulty, volume);

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
    use crate::ir::events::{OperandEvent, OperatorEvent};

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
        assert_eq!(r.halstead.distinct_operands, 2); // "a", "b"
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
