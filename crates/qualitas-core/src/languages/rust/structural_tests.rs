use super::RustAdapter;
use crate::analyzer::analyze_source_str;
use crate::ir::language::LanguageAdapter;
use crate::metrics::structural::{compute_sm_from_events, SourceSpan};
use crate::types::AnalysisOptions;

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
        &SourceSpan {
            source,
            start: func.byte_start,
            end: func.byte_end,
        },
        func.param_count,
    )
}

fn rs_first_fn_report(source: &str) -> crate::types::FunctionQualityReport {
    let report = analyze_source_str(source, "test.rs", &AnalysisOptions::default()).unwrap();
    report
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function in Rust report")
}

fn rs_first_fn_statement_count(source: &str) -> Option<u32> {
    let adapter = RustAdapter;
    let extraction = adapter.extract(source, "test.rs").unwrap();
    extraction
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function")
        .statement_count
}

// ── Basic structural metrics ────────────────────────────────────────────────

#[test]
fn rs_empty_function_has_zero_metrics() {
    let source = r"
fn empty() {}
";
    let sm = rs_first_fn_sm(source);
    assert_eq!(sm.parameter_count, 0);
    assert_eq!(sm.return_count, 0);
}

#[test]
fn rs_counts_params_and_returns() {
    let source = r"
fn add(a: i32, b: i32, c: i32) -> i32 {
    return a + b + c;
}
";
    let sm = rs_first_fn_sm(source);
    assert_eq!(sm.parameter_count, 3);
    assert_eq!(sm.return_count, 1);
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

// ── Logical LOC (statement_count) tests ─────────────────────────────────────

#[test]
fn rs_statement_count_single_return() {
    let source = r"
fn add(a: i32, b: i32) -> i32 {
    a + b
}
";
    assert_eq!(
        rs_first_fn_statement_count(source),
        Some(1),
        "Single expression should give statement_count = 1",
    );
}

#[test]
fn rs_statement_count_empty_function() {
    let source = r"
fn empty() {}
";
    assert_eq!(rs_first_fn_statement_count(source), Some(0));
}

#[test]
fn rs_statement_count_includes_nested_block_statements() {
    let source = r"
fn check(x: i32) -> i32 {
    if x > 0 {
        let y = x * 2;
        return y;
    }
    x
}
";
    // 4 statements: if (+ nested let y, return y), x
    assert_eq!(
        rs_first_fn_statement_count(source),
        Some(4),
        "Expected 4 statements (including nested)",
    );
}

#[test]
fn rs_statement_count_match_arms() {
    let source = r#"
fn describe(x: i32) -> &'static str {
    match x {
        1 => "one",
        2 => "two",
        _ => "other",
    }
}
"#;
    // 4 statements: match + 3 arms
    assert_eq!(
        rs_first_fn_statement_count(source),
        Some(4),
        "Expected 4 statements (match + 3 arms)",
    );
}

#[test]
fn rs_statement_count_for_loop_body() {
    let source = r"
fn sum(items: &[i32]) -> i32 {
    let mut total = 0;
    for item in items {
        if *item > 0 {
            total += item;
        }
    }
    total
}
";
    // 5 statements: let total, for (+ if (+ total += )), total
    assert_eq!(
        rs_first_fn_statement_count(source),
        Some(5),
        "Expected 5 statements (top-level + nested)",
    );
}

// ── Logical LOC flows through analysis pipeline ─────────────────────────────

#[test]
fn rs_logical_loc_used_in_structural_metric() {
    let source = r"
fn add(a: i32, b: i32) -> i32 {
    a + b
}
";
    let report = rs_first_fn_report(source);
    assert_eq!(
        report.metrics.structural.loc, 1,
        "Structural metric should use logical LOC (1 statement), got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn rs_logical_loc_includes_nested_statements() {
    let source = r"
fn check(x: i32) -> i32 {
    if x > 0 {
        let y = x * 2;
        return y;
    }
    x
}
";
    let report = rs_first_fn_report(source);
    assert_eq!(
        report.metrics.structural.loc, 4,
        "Logical LOC should include nested statements, got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn rs_physical_loc_in_total_lines() {
    let source = r"
fn add(a: i32, b: i32) -> i32 {
    a + b
}
";
    let report = rs_first_fn_report(source);
    assert!(
        report.metrics.structural.total_lines >= 3,
        "total_lines should be >= 3 physical lines, got {}",
        report.metrics.structural.total_lines,
    );
}
