use crate::analyzer::analyze_source_str;
use crate::ir::events::QualitasEvent;
use crate::metrics::cognitive_flow::compute_cfc;
use crate::types::AnalysisOptions;

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

#[test]
fn rs_early_return_beats_if_else() {
    let early_return = r#"
fn grade(score: f64) -> &'static str {
    if score >= 80.0 { return "A"; }
    if score >= 65.0 { return "B"; }
    if score >= 50.0 { return "C"; }
    if score >= 35.0 { return "D"; }
    "F"
}
"#;
    let if_else = r#"
fn grade(score: f64) -> &'static str {
    if score >= 80.0 {
        "A"
    } else if score >= 65.0 {
        "B"
    } else if score >= 50.0 {
        "C"
    } else if score >= 35.0 {
        "D"
    } else {
        "F"
    }
}
"#;
    let opts = AnalysisOptions::default();
    let early_score = analyze_source_str(early_return, "early.rs", &opts)
        .unwrap()
        .functions[0]
        .score;
    let ifelse_score = analyze_source_str(if_else, "ifelse.rs", &opts)
        .unwrap()
        .functions[0]
        .score;

    assert!(
        early_score > ifelse_score,
        "Early return ({early_score:.1}) should score higher than if/else ({ifelse_score:.1})",
    );
}

#[test]
fn rs_match_arms_cost_less_than_if_else() {
    let match_version = r#"
fn classify(x: i32) -> &'static str {
    match x {
        0..=9 => "small",
        10..=99 => "medium",
        100..=999 => "large",
        _ => "huge",
    }
}
"#;
    let if_else_version = r#"
fn classify(x: i32) -> &'static str {
    if x <= 9 {
        "small"
    } else if x <= 99 {
        "medium"
    } else if x <= 999 {
        "large"
    } else {
        "huge"
    }
}
"#;
    let opts = AnalysisOptions::default();
    let match_score = analyze_source_str(match_version, "match.rs", &opts)
        .unwrap()
        .functions[0]
        .score;
    let ifelse_score = analyze_source_str(if_else_version, "ifelse.rs", &opts)
        .unwrap()
        .functions[0]
        .score;

    assert!(
        match_score > ifelse_score,
        "Match ({match_score:.1}) should score higher than if/else ({ifelse_score:.1}) — arms are discounted",
    );
}
