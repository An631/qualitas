use super::JavaAdapter;
use crate::analyzer::analyze_source_str;
use crate::ir::language::LanguageAdapter;
use crate::metrics::structural::{compute_sm_from_events, SourceSpan};
use crate::types::AnalysisOptions;

/// Helper: compute raw SM from the first Java method.
fn java_first_method_sm(source: &str) -> crate::types::StructuralResult {
    let adapter = JavaAdapter;
    let extraction = adapter.extract(source, "Test.java").unwrap();
    let method = extraction
        .classes
        .into_iter()
        .next()
        .expect("Expected a class")
        .methods
        .into_iter()
        .next()
        .expect("Expected a method");
    compute_sm_from_events(
        &method.events,
        &SourceSpan {
            source,
            start: method.byte_start,
            end: method.byte_end,
        },
        method.param_count,
    )
}

/// Helper: get the full report for the first method via the analysis pipeline.
fn java_first_fn_report(source: &str) -> crate::types::FunctionQualityReport {
    let report = analyze_source_str(source, "Test.java", &AnalysisOptions::default()).unwrap();
    // Java methods live inside classes, not top-level functions
    if !report.functions.is_empty() {
        return report.functions.into_iter().next().unwrap();
    }
    report
        .classes
        .into_iter()
        .next()
        .expect("Expected at least one class in Java report")
        .methods
        .into_iter()
        .next()
        .expect("Expected at least one method in Java class report")
}

/// Helper: get statement_count directly from the extraction.
fn java_first_method_statement_count(source: &str) -> Option<u32> {
    let adapter = JavaAdapter;
    let extraction = adapter.extract(source, "Test.java").unwrap();
    extraction
        .classes
        .into_iter()
        .next()
        .expect("Expected a class")
        .methods
        .into_iter()
        .next()
        .expect("Expected a method")
        .statement_count
}

// ── Basic structural metrics ─────────────────────────────────────────────────

#[test]
fn java_empty_method_has_zero_metrics() {
    let source = r"
public class Test {
    public void empty() {
    }
}
";
    let sm = java_first_method_sm(source);
    assert_eq!(sm.parameter_count, 0);
    assert_eq!(sm.return_count, 0);
}

#[test]
fn java_counts_params_and_returns() {
    let source = r"
public class Test {
    public int add(int a, int b, int c) {
        return a + b + c;
    }
}
";
    let sm = java_first_method_sm(source);
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
fn java_counts_nesting_depth() {
    let source = r"
import java.util.List;

public class Test {
    public int nested(List<Integer> data) {
        if (data.size() > 0) {
            for (int item : data) {
                return item;
            }
        }
        return 0;
    }
}
";
    let sm = java_first_method_sm(source);
    assert!(
        sm.max_nesting_depth >= 2,
        "Expected nesting depth >= 2, got {}",
        sm.max_nesting_depth,
    );
}

// ── Logical LOC (statement_count) tests ──────────────────────────────────────

#[test]
fn java_statement_count_single_return() {
    let source = r"
public class Test {
    public int add(int a, int b) {
        return a + b;
    }
}
";
    let stmt_count = java_first_method_statement_count(source);
    assert_eq!(
        stmt_count,
        Some(1),
        "Single return statement should give statement_count = 1, got {stmt_count:?}",
    );
}

#[test]
fn java_statement_count_multiple_statements() {
    let source = r"
public class Test {
    public int process(int x) {
        int y = x * 2;
        int z = y + 1;
        if (z > 10) {
            return z;
        }
        return y;
    }
}
";
    let stmt_count = java_first_method_statement_count(source);
    // 5 statements: y=, z=, if (+ nested return z), return y
    assert_eq!(
        stmt_count,
        Some(5),
        "Expected 5 statements (including nested return), got {stmt_count:?}",
    );
}

#[test]
fn java_statement_count_empty_method() {
    let source = r"
public class Test {
    public void empty() {
    }
}
";
    let stmt_count = java_first_method_statement_count(source);
    assert_eq!(
        stmt_count,
        Some(0),
        "Empty method should have statement_count = 0, got {stmt_count:?}",
    );
}

#[test]
fn java_statement_count_includes_nested_blocks() {
    let source = r"
public class Test {
    public String check(int x) {
        if (x > 0) {
            int y = x * 2;
            return String.valueOf(y);
        }
        return String.valueOf(x);
    }
}
";
    let stmt_count = java_first_method_statement_count(source);
    // 4 statements: if, y=, inner return, outer return
    assert_eq!(
        stmt_count,
        Some(4),
        "Expected 4 statements (including nested), got {stmt_count:?}",
    );
}

#[test]
fn java_statement_count_for_loop_body() {
    let source = r"
public class Test {
    public int sum(int n) {
        int total = 0;
        for (int i = 0; i < n; i++) {
            total += i;
        }
        return total;
    }
}
";
    let stmt_count = java_first_method_statement_count(source);
    // 4 statements: total=, for, total+=, return
    assert_eq!(
        stmt_count,
        Some(4),
        "Expected 4 statements, got {stmt_count:?}",
    );
}

#[test]
fn java_statement_count_try_catch() {
    let source = r#"
public class Test {
    public String safe(String s) {
        try {
            return s.toUpperCase();
        } catch (Exception e) {
            return "error";
        }
    }
}
"#;
    let stmt_count = java_first_method_statement_count(source);
    // 3 statements: try, return in try body, return in catch body
    assert_eq!(
        stmt_count,
        Some(3),
        "Expected 3 statements (try + 2 returns), got {stmt_count:?}",
    );
}

// ── Logical LOC flows through analysis pipeline ──────────────────────────────

#[test]
fn java_logical_loc_used_in_structural_metric() {
    let source = r"
public class Test {
    public int add(int a, int b) {
        return a + b;
    }
}
";
    let report = java_first_fn_report(source);
    assert_eq!(
        report.metrics.structural.loc, 1,
        "Structural metric should use logical LOC (1 statement), got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn java_logical_loc_includes_nested_statements() {
    let source = r"
public class Test {
    public int process(int x) {
        int y = x * 2;
        if (y > 10) {
            return y;
        }
        return x;
    }
}
";
    let report = java_first_fn_report(source);
    // 4 statements: y=, if, inner return, outer return
    assert_eq!(
        report.metrics.structural.loc, 4,
        "Expected 4 logical LOC, got {}",
        report.metrics.structural.loc,
    );
}

#[test]
fn java_physical_loc_in_total_lines() {
    let source = r"
public class Test {
    public int add(int a, int b) {
        return a + b;
    }
}
";
    let report = java_first_fn_report(source);
    assert!(
        report.metrics.structural.total_lines >= 3,
        "total_lines should be >= 3 physical lines, got {}",
        report.metrics.structural.total_lines,
    );
}

#[test]
fn java_nested_class_methods_have_separate_loc() {
    let source = r"
public class Outer {
    public void outerMethod() {
        int x = 1;
        int y = 2;
    }

    public static class Inner {
        public void innerMethod() {
            int z = 3;
        }
    }
}
";
    let adapter = JavaAdapter;
    let extraction = adapter.extract(source, "Outer.java").unwrap();

    let inner = extraction
        .classes
        .iter()
        .find(|c| c.name.contains("Inner"))
        .unwrap();
    let inner_method = &inner.methods[0];
    assert_eq!(
        inner_method.statement_count,
        Some(1),
        "Inner class method should have 1 statement, got {:?}",
        inner_method.statement_count,
    );
}

#[test]
fn java_anonymous_class_method_lloc() {
    let source = r"
import java.util.Comparator;

public class Test {
    public void sort(java.util.List<String> items) {
        items.sort(new Comparator<String>() {
            public int compare(String a, String b) {
                int result = a.compareTo(b);
                return result;
            }
        });
    }
}
";
    let adapter = JavaAdapter;
    let extraction = adapter.extract(source, "Test.java").unwrap();

    let anon_class = extraction
        .classes
        .iter()
        .find(|c| c.name.contains("anonymous"));
    assert!(
        anon_class.is_some(),
        "Expected anonymous class extraction, classes: {:?}",
        extraction
            .classes
            .iter()
            .map(|c| &c.name)
            .collect::<Vec<_>>(),
    );
    let compare = &anon_class.unwrap().methods[0];
    assert_eq!(
        compare.statement_count,
        Some(2),
        "Anonymous class compare method should have 2 statements, got {:?}",
        compare.statement_count,
    );
}

#[test]
fn java_anonymous_class_not_in_parent_lloc() {
    let source = r"
import java.util.Comparator;

public class Test {
    public void sort(java.util.List<String> items) {
        items.sort(new Comparator<String>() {
            public int compare(String a, String b) {
                int result = a.compareTo(b);
                return result;
            }
        });
    }
}
";
    let adapter = JavaAdapter;
    let extraction = adapter.extract(source, "Test.java").unwrap();

    let test_class = extraction
        .classes
        .iter()
        .find(|c| c.name == "Test")
        .unwrap();
    let sort_method = &test_class.methods[0];
    // The sort method body has 1 statement: the expression statement `items.sort(...)`.
    // Anonymous class body statements should NOT be counted in the parent.
    assert_eq!(
        sort_method.statement_count,
        Some(1),
        "Parent sort method should have 1 statement (the method call), got {:?}",
        sort_method.statement_count,
    );
}
