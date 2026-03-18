use super::GoAdapter;
use crate::analyzer::analyze_source_str;
use crate::ir::language::LanguageAdapter;
use crate::metrics::structural::{compute_sm_from_events, SourceSpan};
use crate::types::AnalysisOptions;

/// Helper: compute raw SM from the first Go function (physical LOC path).
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

/// Helper: get the full report for the first function via the analysis pipeline
/// (applies logical LOC override from statement_count).
fn go_first_fn_report(source: &str) -> crate::types::FunctionQualityReport {
    let report = analyze_source_str(source, "test.go", &AnalysisOptions::default()).unwrap();
    report
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function in Go report")
}

/// Helper: get statement_count directly from the extraction.
fn go_first_fn_statement_count(source: &str) -> Option<u32> {
    let adapter = GoAdapter;
    let extraction = adapter.extract(source, "test.go").unwrap();
    extraction
        .functions
        .into_iter()
        .next()
        .expect("Expected at least one function")
        .statement_count
}

// ── Basic structural metrics ────────────────────────────────────────────────

#[test]
fn go_empty_function_has_zero_metrics() {
    let source = r"
package main

func empty() {
}
";
    let sm = go_first_fn_sm(source);
    assert_eq!(sm.parameter_count, 0);
    assert_eq!(sm.return_count, 0);
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
        sm.parameter_count
    );
    assert_eq!(
        sm.return_count, 1,
        "Expected 1 return, got {}",
        sm.return_count
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
        sm.parameter_count
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
        sm.return_count
    );
}

// ── Logical LOC (statement_count) tests ─────────────────────────────────────

#[test]
fn go_statement_count_single_return() {
    let source = r"
package main

func add(a, b int) int {
    return a + b
}
";
    let stmt_count = go_first_fn_statement_count(source);
    assert_eq!(
        stmt_count,
        Some(1),
        "Single return statement should give statement_count = 1, got {stmt_count:?}",
    );
}

#[test]
fn go_statement_count_multiple_statements() {
    let source = r"
package main

func process(x int) int {
    y := x * 2
    z := y + 1
    if z > 10 {
        return z
    }
    return y
}
";
    let stmt_count = go_first_fn_statement_count(source);
    // 3 top-level statements: y :=, z :=, if, return
    assert_eq!(
        stmt_count,
        Some(4),
        "Expected 4 top-level statements, got {stmt_count:?}",
    );
}

#[test]
fn go_statement_count_empty_function() {
    let source = r"
package main

func empty() {
}
";
    let stmt_count = go_first_fn_statement_count(source);
    assert_eq!(
        stmt_count,
        Some(0),
        "Empty function should have statement_count = 0, got {stmt_count:?}",
    );
}

#[test]
fn go_statement_count_excludes_nested_block_statements() {
    // statement_count counts only top-level statements in the function body,
    // not statements inside if/for/switch blocks.
    let source = r"
package main

func check(x int) string {
    if x > 0 {
        y := x * 2
        return fmt.Sprintf(y)
    }
    return fmt.Sprintf(x)
}
";
    let stmt_count = go_first_fn_statement_count(source);
    // Top-level statements: if, return — the y := and inner return are nested
    assert_eq!(
        stmt_count,
        Some(2),
        "Expected 2 top-level statements (if + return), got {stmt_count:?}",
    );
}

// ── Logical LOC flows through analysis pipeline ─────────────────────────────

#[test]
fn go_logical_loc_used_in_structural_metric() {
    // The structural metric should use logical LOC (statement_count),
    // not physical LOC (line count).
    let source = r"
package main

func add(a, b int) int {
    return a + b
}
";
    let report = go_first_fn_report(source);
    // Logical LOC = 1 (one statement: return)
    assert_eq!(
        report.metrics.structural.loc, 1,
        "Structural metric should use logical LOC (1 statement), got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn go_physical_loc_in_total_lines() {
    // total_lines should reflect the physical line count of the function span.
    let source = r"
package main

func add(a, b int) int {
    return a + b
}
";
    let report = go_first_fn_report(source);
    // Physical lines: `func add(a, b int) int {`, `    return a + b`, `}`  → 3 lines
    assert!(
        report.metrics.structural.total_lines >= 3,
        "total_lines should be >= 3 physical lines, got {}",
        report.metrics.structural.total_lines,
    );
}

#[test]
fn go_logical_loc_differs_from_physical_loc() {
    // A multi-line function where logical LOC (statements) differs from physical LOC (lines).
    let source = r"
package main

func process(
    x int,
    y int,
    z int,
) int {
    result := x + y + z
    return result
}
";
    let report = go_first_fn_report(source);
    // Logical LOC = 2 (two statements: assignment + return)
    assert_eq!(
        report.metrics.structural.loc, 2,
        "Logical LOC should be 2 statements, got {}",
        report.metrics.structural.loc,
    );
    // Physical lines should be much higher (signature spans ~5 lines + body)
    assert!(
        report.metrics.structural.total_lines > 2,
        "Physical lines should exceed logical LOC, got {}",
        report.metrics.structural.total_lines,
    );
}

#[test]
fn go_error_handling_logical_loc() {
    // Go error handling inflates physical LOC but each `if err != nil` is just 1 statement.
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
    err = step3()
    if err != nil {
        return err
    }
    return nil
}
";
    let report = go_first_fn_report(source);
    // Top-level statements: err:=, if, err=, if, err=, if, return → 7
    assert_eq!(
        report.metrics.structural.loc, 7,
        "Logical LOC should be 7 top-level statements, got {}",
        report.metrics.structural.loc,
    );
    // Physical LOC is much higher because each if block spans 3 lines
    assert!(
        report.metrics.structural.total_lines > 7,
        "Physical LOC ({}) should exceed logical LOC (7)",
        report.metrics.structural.total_lines,
    );
}

#[test]
fn go_method_logical_loc() {
    let source = r"
package main

type Server struct{}

func (s *Server) Handle(req Request) Response {
    if req.Method == GET {
        return s.handleGet(req)
    }
    return s.handleDefault(req)
}
";
    let adapter = GoAdapter;
    let extraction = adapter.extract(source, "test.go").unwrap();
    let method = &extraction.classes[0].methods[0];
    assert_eq!(
        method.statement_count,
        Some(2),
        "Method should have 2 top-level statements (if + return), got {:?}",
        method.statement_count,
    );
}
