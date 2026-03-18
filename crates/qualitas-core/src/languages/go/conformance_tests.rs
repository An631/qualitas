use super::GoAdapter;
use crate::ir::events::QualitasEvent;
use crate::ir::language::LanguageAdapter;

fn check_conformance(adapter: &dyn LanguageAdapter, source: &str, file_name: &str) {
    let extraction = adapter.extract(source, file_name).unwrap_or_else(|e| {
        panic!(
            "Adapter {} failed to parse {file_name}: {e}",
            adapter.name()
        )
    });

    for func in &extraction.functions {
        check_function_conformance(adapter.name(), func);
    }
    for class in &extraction.classes {
        for method in &class.methods {
            check_function_conformance(adapter.name(), method);
        }
    }
}

fn check_function_conformance(adapter_name: &str, func: &crate::ir::language::FunctionExtraction) {
    assert!(
        !func.name.is_empty(),
        "[{adapter_name}] Function has empty name at byte {}-{}",
        func.byte_start,
        func.byte_end,
    );

    assert!(
        func.start_line > 0,
        "[{adapter_name}] {}: start_line is 0",
        func.name,
    );
    assert!(
        func.start_line <= func.end_line,
        "[{adapter_name}] {}: start_line ({}) > end_line ({})",
        func.name,
        func.start_line,
        func.end_line,
    );

    let nesting_enters = func
        .events
        .iter()
        .filter(|e| matches!(e, QualitasEvent::NestingEnter))
        .count();
    let nesting_exits = func
        .events
        .iter()
        .filter(|e| matches!(e, QualitasEvent::NestingExit))
        .count();
    assert_eq!(
        nesting_enters, nesting_exits,
        "[{adapter_name}] {}: unbalanced NestingEnter ({nesting_enters}) vs NestingExit ({nesting_exits})",
        func.name,
    );

    let nested_fn_enters = func
        .events
        .iter()
        .filter(|e| matches!(e, QualitasEvent::NestedFunctionEnter))
        .count();
    let nested_fn_exits = func
        .events
        .iter()
        .filter(|e| matches!(e, QualitasEvent::NestedFunctionExit))
        .count();
    assert_eq!(
        nested_fn_enters, nested_fn_exits,
        "[{adapter_name}] {}: unbalanced NestedFunctionEnter ({nested_fn_enters}) vs NestedFunctionExit ({nested_fn_exits})",
        func.name,
    );
}

#[test]
fn go_clean_code_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r#"
package main

func add(a, b int) int {
    return a + b
}

func capitalize(s string) string {
    if s == "" {
        return ""
    }
    return string(s[0]-32) + s[1:]
}
"#,
        "clean.go",
    );
}

#[test]
fn go_complex_code_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r#"
package main

import "fmt"

func processOrders(orders []Order, config Config) []bool {
    results := make([]bool, 0)
    for _, order := range orders {
        if order.Status == "pending" {
            if len(order.Items) > 0 {
                for _, item := range order.Items {
                    if item.Quantity > 0 {
                        results = append(results, true)
                    }
                }
            }
        }
    }
    fmt.Println("done")
    return results
}
"#,
        "complex.go",
    );
}

#[test]
fn go_method_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r"
package main

type Calculator struct{}

func (c *Calculator) Add(a, b int) int {
    return a + b
}

func (c *Calculator) Subtract(a, b int) int {
    return a - b
}
",
        "method.go",
    );
}

#[test]
fn go_goroutine_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r"
package main

func process(items []int) {
    go func() {
        for _, item := range items {
            _ = item
        }
    }()
}
",
        "goroutine.go",
    );
}

#[test]
fn go_defer_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r"
package main

func readFile(path string) string {
    f := open(path)
    defer f.Close()
    return f.Read()
}
",
        "defer.go",
    );
}

#[test]
fn go_select_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r"
package main

func multiplex(ch1, ch2 chan int) int {
    select {
    case v := <-ch1:
        return v
    case v := <-ch2:
        return v
    }
}
",
        "select.go",
    );
}

#[test]
fn go_switch_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r#"
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
"#,
        "switch.go",
    );
}

#[test]
fn go_empty_source_conforms() {
    let adapter = GoAdapter;
    let extraction = adapter.extract("package main\n", "empty.go").unwrap();
    assert!(extraction.functions.is_empty());
    assert!(extraction.classes.is_empty());
}

#[test]
fn go_syntax_error_returns_err() {
    let adapter = GoAdapter;
    let result = adapter.extract("func (((:", "bad.go");
    assert!(result.is_err());
}

#[test]
fn go_if_err_nil_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r"
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
",
        "error_handling.go",
    );
}

#[test]
fn go_clean_function_scores_high() {
    let source = r"
package main

func add(a, b int) int {
    return a + b
}
";
    let report = crate::analyzer::analyze_source_str(
        source,
        "clean.go",
        &crate::types::AnalysisOptions::default(),
    )
    .unwrap();
    assert!(
        report.score >= 80.0,
        "Expected score >= 80, got {:.2}",
        report.score
    );
    assert_eq!(report.grade, crate::types::Grade::A);
    assert!(!report.needs_refactoring);
}

#[test]
fn go_type_switch_conforms() {
    let adapter = GoAdapter;
    check_conformance(
        &adapter,
        r#"
package main

func describe(i interface{}) string {
    switch i.(type) {
    case int:
        return "int"
    case string:
        return "string"
    default:
        return "unknown"
    }
}
"#,
        "type_switch.go",
    );
}

#[test]
fn go_receiver_param_stripped() {
    let adapter = GoAdapter;
    let extraction = adapter
        .extract(
            r"
package main

type Foo struct{}

func (f *Foo) Bar(x, y int) int {
    return x + y
}
",
            "method.go",
        )
        .unwrap();
    let method = &extraction.classes[0].methods[0];
    assert_eq!(
        method.param_count, 2,
        "Method param_count should exclude receiver, got {}",
        method.param_count,
    );
}
