use super::PythonAdapter;
use crate::ir::language::LanguageAdapter;
use crate::metrics::structural::{compute_sm_from_events, SourceSpan};

fn py_first_fn_sm(source: &str) -> crate::types::StructuralResult {
    let adapter = PythonAdapter;
    let extraction = adapter.extract(source, "test.py").unwrap();
    let func = extraction
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function in Python source");
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
fn py_empty_function_has_zero_metrics() {
    let source = r"
def empty():
    pass
";
    let sm = py_first_fn_sm(source);
    assert_eq!(
        sm.parameter_count, 0,
        "Empty Python function should have 0 params, got {}",
        sm.parameter_count,
    );
    assert_eq!(
        sm.return_count, 0,
        "Empty Python function should have 0 returns, got {}",
        sm.return_count,
    );
}

#[test]
fn py_counts_params_and_returns() {
    let source = r"
def add(a, b, c):
    return a + b + c
";
    let sm = py_first_fn_sm(source);
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
fn py_counts_nesting_depth() {
    let source = r"
def nested(data):
    if data:
        for item in data:
            return item
    return None
";
    let sm = py_first_fn_sm(source);
    assert!(
        sm.max_nesting_depth >= 2,
        "Expected nesting depth >= 2, got {}",
        sm.max_nesting_depth,
    );
}

#[test]
fn py_counts_default_params() {
    let source = r#"
def greet(name, greeting="Hello"):
    return f"{greeting}, {name}"
"#;
    let sm = py_first_fn_sm(source);
    assert_eq!(
        sm.parameter_count, 2,
        "Expected 2 params (including default), got {}",
        sm.parameter_count,
    );
}

#[test]
fn py_counts_star_params() {
    let source = r"
def variadic(*args, **kwargs):
    return args, kwargs
";
    let sm = py_first_fn_sm(source);
    assert_eq!(
        sm.parameter_count, 2,
        "Expected 2 params (*args and **kwargs), got {}",
        sm.parameter_count,
    );
}
