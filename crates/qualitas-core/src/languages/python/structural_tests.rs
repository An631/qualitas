use super::PythonAdapter;
use crate::analyzer::analyze_source_str;
use crate::ir::language::LanguageAdapter;
use crate::metrics::structural::{compute_sm_from_events, SourceSpan};
use crate::types::AnalysisOptions;

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

fn py_first_fn_report(source: &str) -> crate::types::FunctionQualityReport {
    let report = analyze_source_str(source, "test.py", &AnalysisOptions::default()).unwrap();
    report
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function in Python report")
}

fn py_first_fn_statement_count(source: &str) -> Option<u32> {
    let adapter = PythonAdapter;
    let extraction = adapter.extract(source, "test.py").unwrap();
    extraction
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function")
        .statement_count
}

// ── Basic structural metrics ────────────────────────────────────────────────

#[test]
fn py_empty_function_has_zero_metrics() {
    let source = r"
def empty():
    pass
";
    let sm = py_first_fn_sm(source);
    assert_eq!(sm.parameter_count, 0);
    assert_eq!(sm.return_count, 0);
}

#[test]
fn py_counts_params_and_returns() {
    let source = r"
def add(a, b, c):
    return a + b + c
";
    let sm = py_first_fn_sm(source);
    assert_eq!(sm.parameter_count, 3);
    assert_eq!(sm.return_count, 1);
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
    assert_eq!(sm.parameter_count, 2);
}

#[test]
fn py_counts_star_params() {
    let source = r"
def variadic(*args, **kwargs):
    return args, kwargs
";
    let sm = py_first_fn_sm(source);
    assert_eq!(sm.parameter_count, 2);
}

// ── Logical LOC (statement_count) tests ─────────────────────────────────────

#[test]
fn py_statement_count_single_return() {
    let source = r"
def add(a, b):
    return a + b
";
    assert_eq!(
        py_first_fn_statement_count(source),
        Some(1),
        "Single return should give statement_count = 1",
    );
}

#[test]
fn py_statement_count_empty_function() {
    let source = r"
def empty():
    pass
";
    assert_eq!(
        py_first_fn_statement_count(source),
        Some(1),
        "pass is one statement",
    );
}

#[test]
fn py_statement_count_includes_nested_block_statements() {
    let source = r"
def check(x):
    if x > 0:
        y = x * 2
        return y
    return x
";
    // 4 statements: if, y =, inner return, outer return
    assert_eq!(
        py_first_fn_statement_count(source),
        Some(4),
        "Expected 4 statements (including nested)",
    );
}

#[test]
fn py_statement_count_for_loop_body() {
    let source = r"
def process(items):
    total = 0
    for item in items:
        if item > 0:
            total += item
    return total
";
    // 6 statements: total=, for, if, total+=, (no else), return
    // top-level: total=, for, return = 3; nested: if, total+= = 2
    assert_eq!(
        py_first_fn_statement_count(source),
        Some(5),
        "Expected 5 statements (top-level + nested)",
    );
}

#[test]
fn py_statement_count_try_except() {
    let source = r"
def safe_parse(text):
    try:
        return int(text)
    except ValueError:
        return 0
";
    // 3 statements: try(1) + return int(text)(1) + return 0(1)
    // except is a clause of try, not a separate statement
    assert_eq!(
        py_first_fn_statement_count(source),
        Some(3),
        "Expected 3 statements (try + nested returns in try/except bodies)",
    );
}

#[test]
fn py_statement_count_if_elif_else() {
    let source = r#"
def grade(score):
    if score >= 80:
        return "A"
    elif score >= 65:
        return "B"
    else:
        return "C"
"#;
    // 4 statements: if(1) + return A(1) + return B(1) + return C(1)
    // elif/else are clauses of if, not separate statements
    assert_eq!(
        py_first_fn_statement_count(source),
        Some(4),
        "Expected 4 statements (if + nested returns in all branches)",
    );
}

// ── Logical LOC flows through analysis pipeline ─────────────────────────────

#[test]
fn py_logical_loc_used_in_structural_metric() {
    let source = r"
def add(a, b):
    return a + b
";
    let report = py_first_fn_report(source);
    assert_eq!(
        report.metrics.structural.loc, 1,
        "Structural metric should use logical LOC (1 statement), got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn py_logical_loc_includes_nested_statements() {
    let source = r"
def process(x):
    if x > 0:
        y = x * 2
        return y
    return x
";
    let report = py_first_fn_report(source);
    assert_eq!(
        report.metrics.structural.loc, 4,
        "Logical LOC should include nested statements, got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn py_physical_loc_in_total_lines() {
    let source = r"
def add(a, b):
    return a + b
";
    let report = py_first_fn_report(source);
    assert!(
        report.metrics.structural.total_lines >= 2,
        "total_lines should be >= 2 physical lines, got {}",
        report.metrics.structural.total_lines,
    );
}
