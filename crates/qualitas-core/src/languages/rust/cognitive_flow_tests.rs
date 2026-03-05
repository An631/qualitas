use crate::ir::events::QualitasEvent;
use crate::metrics::cognitive_flow::compute_cfc;

#[test]
fn rs_match_with_3_arms_has_cfc() {
    let source = r#"
fn describe(value: Option<i32>) -> &'static str {
    match value {
        Some(n) if n > 0 => "positive",
        Some(_) => "other",
        None => "nothing",
    }
}
"#;
    let events = super::rs_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score > 0,
        "Rust match with 3 arms should have CFC > 0, got {}",
        cfc.score,
    );
}

#[test]
fn rs_for_loop_increments_cfc() {
    let source = r"
fn sum(items: &[i32]) -> i32 {
    let mut total = 0;
    for item in items {
        total += item;
    }
    total
}
";
    let events = super::rs_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Rust for loop should increment CFC to at least 1, got {}",
        cfc.score,
    );
    let has_for = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::ForOf,
                ..
            })
        )
    });
    assert!(
        has_for,
        "Expected a ForOf control flow event for Rust for loop"
    );
}
