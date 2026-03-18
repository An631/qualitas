use crate::ir::events::QualitasEvent;
use crate::metrics::cognitive_flow::compute_cfc;

#[test]
fn go_if_else_has_cfc() {
    let source = r#"
package main

func check(x int) string {
    if x > 0 {
        return "positive"
    } else if x < 0 {
        return "negative"
    } else {
        return "zero"
    }
}
"#;
    let events = super::go_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score > 0,
        "Go if/else should have CFC > 0, got {}",
        cfc.score,
    );
}

#[test]
fn go_for_range_increments_cfc() {
    let source = r"
package main

func sumList(items []int) int {
    total := 0
    for _, item := range items {
        total += item
    }
    return total
}
";
    let events = super::go_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Go for range should increment CFC to at least 1, got {}",
        cfc.score,
    );
    let has_for_of = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::ForOf,
                ..
            })
        )
    });
    assert!(
        has_for_of,
        "Expected a ForOf control flow event for Go for range"
    );
}

#[test]
fn go_c_style_for_increments_cfc() {
    let source = r"
package main

func countdown(n int) int {
    for i := n; i > 0; i-- {
        n = i
    }
    return n
}
";
    let events = super::go_first_fn_events(source);
    let has_for = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::For,
                ..
            })
        )
    });
    assert!(has_for, "Expected a For control flow event for C-style for");
}

#[test]
fn go_bare_for_is_while() {
    let source = r"
package main

func spin() {
    for {
        break
    }
}
";
    let events = super::go_first_fn_events(source);
    let has_while = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::While,
                ..
            })
        )
    });
    assert!(
        has_while,
        "Expected a While control flow event for bare for loop"
    );
}

#[test]
fn go_condition_for_is_while() {
    let source = r"
package main

func countdown(n int) int {
    for n > 0 {
        n--
    }
    return n
}
";
    let events = super::go_first_fn_events(source);
    let has_while = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::While,
                ..
            })
        )
    });
    assert!(
        has_while,
        "Expected a While control flow event for condition-only for"
    );
}

#[test]
fn go_switch_increments_cfc() {
    let source = r#"
package main

func classify(x int) string {
    switch {
    case x > 0:
        return "positive"
    case x < 0:
        return "negative"
    default:
        return "zero"
    }
}
"#;
    let events = super::go_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Go switch should have CFC >= 1, got {}",
        cfc.score,
    );
}

#[test]
fn go_select_increments_cfc() {
    let source = r"
package main

func multiplex(ch1, ch2 chan int) int {
    select {
    case v := <-ch1:
        return v
    case v := <-ch2:
        return v
    }
}
";
    let events = super::go_first_fn_events(source);
    let has_ctx = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::ContextManager,
                ..
            })
        )
    });
    assert!(has_ctx, "Expected ContextManager events for Go select");
}

#[test]
fn go_boolean_operators_increment_cfc() {
    let source = r"
package main

func check(a, b, c bool) bool {
    if a && b || c {
        return true
    }
    return false
}
";
    let events = super::go_first_fn_events(source);
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
fn go_goroutine_has_spawn_event() {
    let source = r"
package main

func launch() {
    go func() {}()
}
";
    let events = super::go_first_fn_events(source);
    let has_spawn = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::AsyncComplexity(crate::ir::events::AsyncEvent::Spawn)
        )
    });
    assert!(has_spawn, "Expected Spawn event for go statement");
}

#[test]
fn go_defer_no_cfc() {
    let source = r"
package main

func cleanup() {
    defer func() {
        if true {
            println()
        }
    }()
}
";
    let events = super::go_first_fn_events(source);
    // The if inside defer should NOT contribute ControlFlow (suppress_cfc)
    let cf_count = events
        .iter()
        .filter(|e| matches!(e, QualitasEvent::ControlFlow(_)))
        .count();
    assert_eq!(
        cf_count, 0,
        "Defer body should not emit ControlFlow events, got {cf_count}"
    );
}

#[test]
fn go_error_pattern_counted_normally() {
    let source = r"
package main

func doWork() error {
    err := step1()
    if err != nil {
        return err
    }
    err = step2()
    if err != nil {
        return err
    }
    return nil
}
";
    let events = super::go_first_fn_events(source);
    let if_count = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                    kind: crate::ir::events::ControlFlowKind::If,
                    ..
                })
            )
        })
        .count();
    assert_eq!(
        if_count, 2,
        "Each if err != nil should emit ControlFlow(If), got {if_count}"
    );
}
