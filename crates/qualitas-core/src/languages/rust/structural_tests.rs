use super::RustAdapter;
use crate::ir::language::LanguageAdapter;
use crate::metrics::structural::compute_sm_from_events;

fn rs_first_fn_sm(source: &str) -> crate::types::StructuralResult {
    let adapter = RustAdapter;
    let extraction = adapter.extract(source, "test.rs").unwrap();
    let func = extraction
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function in Rust source");
    compute_sm_from_events(
        &func.events,
        source,
        func.byte_start,
        func.byte_end,
        func.param_count,
    )
}

#[test]
fn rs_empty_function_has_zero_metrics() {
    let source = r"
fn empty() {}
";
    let sm = rs_first_fn_sm(source);
    assert_eq!(
        sm.parameter_count, 0,
        "Empty Rust function should have 0 params, got {}",
        sm.parameter_count,
    );
    assert_eq!(
        sm.return_count, 0,
        "Empty Rust function should have 0 returns, got {}",
        sm.return_count,
    );
}

#[test]
fn rs_counts_params_and_returns() {
    let source = r"
fn add(a: i32, b: i32, c: i32) -> i32 {
    return a + b + c;
}
";
    let sm = rs_first_fn_sm(source);
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
fn rs_counts_nesting_depth() {
    let source = r"
fn nested(x: bool) -> i32 {
    if x {
        for i in 0..10 {
            return i;
        }
    }
    0
}
";
    let sm = rs_first_fn_sm(source);
    assert!(
        sm.max_nesting_depth >= 2,
        "Expected nesting depth >= 2, got {}",
        sm.max_nesting_depth,
    );
}
