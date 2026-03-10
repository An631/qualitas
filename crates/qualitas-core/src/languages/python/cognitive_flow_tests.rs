use crate::analyzer::analyze_source_str;
use crate::ir::events::QualitasEvent;
use crate::metrics::cognitive_flow::compute_cfc;
use crate::types::AnalysisOptions;

#[test]
fn py_if_else_has_cfc() {
    let source = r#"
def check(x):
    if x > 0:
        return "positive"
    elif x < 0:
        return "negative"
    else:
        return "zero"
"#;
    let events = super::py_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score > 0,
        "Python if/elif/else should have CFC > 0, got {}",
        cfc.score,
    );
}

#[test]
fn py_for_loop_increments_cfc() {
    let source = r"
def sum_list(items):
    total = 0
    for item in items:
        total += item
    return total
";
    let events = super::py_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Python for loop should increment CFC to at least 1, got {}",
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
        "Expected a ForOf control flow event for Python for loop"
    );
}

#[test]
fn py_while_loop_increments_cfc() {
    let source = r"
def countdown(n):
    while n > 0:
        n -= 1
    return n
";
    let events = super::py_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Python while loop should have CFC >= 1, got {}",
        cfc.score,
    );
}

#[test]
fn py_try_except_increments_cfc() {
    let source = r"
def safe_parse(text):
    try:
        return int(text)
    except ValueError:
        return 0
";
    let events = super::py_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Python try/except should have CFC >= 1, got {}",
        cfc.score,
    );
}

#[test]
fn py_with_statement_increments_cfc() {
    let source = r"
def read_file(path):
    with open(path) as f:
        return f.read()
";
    let events = super::py_first_fn_events(source);
    let has_ctx = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::ContextManager,
                ..
            })
        )
    });
    assert!(has_ctx, "Expected a ContextManager event for Python with");
}

#[test]
fn py_comprehension_has_control_flow() {
    let source = r"
def evens(data):
    return [x for x in data if x % 2 == 0]
";
    let events = super::py_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "List comprehension should contribute to CFC, got {}",
        cfc.score,
    );
}

#[test]
fn py_boolean_operators_increment_cfc() {
    let source = r"
def check(a, b, c):
    if a and b or c:
        return True
    return False
";
    let events = super::py_first_fn_events(source);
    let has_and = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::LogicOp(crate::ir::events::LogicOpEvent::And)
        )
    });
    let has_or = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::LogicOp(crate::ir::events::LogicOpEvent::Or)
        )
    });
    assert!(has_and, "Expected And logic op");
    assert!(has_or, "Expected Or logic op");
}

#[test]
fn py_early_return_beats_nested_if_else() {
    let early_return = r#"
def grade(score):
    if score >= 80:
        return "A"
    if score >= 65:
        return "B"
    if score >= 50:
        return "C"
    if score >= 35:
        return "D"
    return "F"
"#;
    let if_else = r#"
def grade(score):
    if score >= 80:
        result = "A"
    elif score >= 65:
        result = "B"
    elif score >= 50:
        result = "C"
    elif score >= 35:
        result = "D"
    else:
        result = "F"
    return result
"#;
    let opts = AnalysisOptions::default();
    let early_score = analyze_source_str(early_return, "early.py", &opts)
        .unwrap()
        .functions[0]
        .score;
    let ifelse_score = analyze_source_str(if_else, "ifelse.py", &opts)
        .unwrap()
        .functions[0]
        .score;

    assert!(
        early_score > ifelse_score,
        "Early return ({early_score:.1}) should score higher than if/elif/else ({ifelse_score:.1})",
    );
}
