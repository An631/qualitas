use super::GoAdapter;
use crate::ir::language::LanguageAdapter;
use crate::metrics::structural::{compute_sm_from_events, SourceSpan};

fn go_first_fn_sm(source: &str) -> crate::types::StructuralResult {
    let adapter = GoAdapter;
    let extraction = adapter.extract(source, "test.go").unwrap();
    let func = extraction
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function in Go source");
    compute_sm_from_events(
        &func.events,
        &SourceSpan {
            source,
            start: func.byte_start,
            end: func.byte_end,
        },
        func.param_count,
    )
}

#[test]
fn go_empty_function_has_zero_metrics() {
    let source = r"
package main

func empty() {
}
";
    let sm = go_first_fn_sm(source);
    assert_eq!(
        sm.parameter_count, 0,
        "Empty Go function should have 0 params, got {}",
        sm.parameter_count,
    );
    assert_eq!(
        sm.return_count, 0,
        "Empty Go function should have 0 returns, got {}",
        sm.return_count,
    );
}

#[test]
fn go_counts_params_and_returns() {
    let source = r"
package main

func add(a, b, c int) int {
    return a + b + c
}
";
    let sm = go_first_fn_sm(source);
    assert_eq!(
        sm.parameter_count, 3,
        "Expected 3 params, got {}",
        sm.parameter_count,
    );
    assert_eq!(
        sm.return_count, 1,
        "Expected 1 return, got {}",
        sm.return_count,
    );
}

#[test]
fn go_counts_nesting_depth() {
    let source = r"
package main

func nested(data []int) int {
    if len(data) > 0 {
        for _, item := range data {
            return item
        }
    }
    return 0
}
";
    let sm = go_first_fn_sm(source);
    assert!(
        sm.max_nesting_depth >= 2,
        "Expected nesting depth >= 2, got {}",
        sm.max_nesting_depth,
    );
}

#[test]
fn go_variadic_params_counted() {
    let source = r"
package main

func variadic(args ...int) int {
    return len(args)
}
";
    let sm = go_first_fn_sm(source);
    assert_eq!(
        sm.parameter_count, 1,
        "Expected 1 param (variadic), got {}",
        sm.parameter_count,
    );
}

#[test]
fn go_multiple_return_is_single() {
    let source = r#"
package main

func divide(a, b int) (int, error) {
    if b == 0 {
        return 0, fmt.Errorf("division by zero")
    }
    return a / b, nil
}
"#;
    let sm = go_first_fn_sm(source);
    assert_eq!(
        sm.return_count, 2,
        "Expected 2 returns, got {}",
        sm.return_count,
    );
}
