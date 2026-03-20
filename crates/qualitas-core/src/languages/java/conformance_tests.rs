use std::collections::HashMap;

use super::JavaAdapter;
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
fn java_clean_code_conforms() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r#"
public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public String greet(String name) {
        if (name.isEmpty()) {
            return "Hello!";
        }
        return "Hello, " + name + "!";
    }
}
"#,
        "Calculator.java",
    );
}

#[test]
fn java_complex_code_conforms() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r#"
import java.util.List;
import java.util.ArrayList;

public class OrderProcessor {
    public List<Boolean> processOrders(List<Order> orders) {
        List<Boolean> results = new ArrayList<>();
        for (Order order : orders) {
            if (order.getStatus().equals("pending")) {
                if (order.getItems().size() > 0) {
                    for (Item item : order.getItems()) {
                        if (item.getQuantity() > 0) {
                            results.add(true);
                        }
                    }
                }
            }
        }
        return results;
    }
}
"#,
        "OrderProcessor.java",
    );
}

#[test]
fn java_class_methods_conform() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r"
public class MathUtils {
    public int add(int a, int b) {
        return a + b;
    }

    public int subtract(int a, int b) {
        return a - b;
    }

    public int multiply(int a, int b) {
        return a * b;
    }
}
",
        "MathUtils.java",
    );
}

#[test]
fn java_lambda_conforms() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r"
import java.util.List;

public class Processor {
    public void process(List<Integer> items) {
        items.forEach(item -> {
            if (item > 0) {
                System.out.println(item);
            }
        });
    }
}
",
        "Processor.java",
    );
}

#[test]
fn java_empty_source_conforms() {
    let adapter = JavaAdapter;
    let extraction = adapter.extract("// empty file\n", "Empty.java").unwrap();
    assert!(extraction.classes.is_empty());
    assert!(extraction.functions.is_empty());
}

#[test]
fn java_syntax_error_returns_err() {
    let adapter = JavaAdapter;
    let result = adapter.extract("public class {{{ broken", "Bad.java");
    assert!(result.is_err());
}

#[test]
fn java_clean_function_scores_high() {
    let source = r"
public class Simple {
    public int add(int a, int b) {
        return a + b;
    }
}
";
    let report = crate::analyzer::analyze_source_str(
        source,
        "Simple.java",
        &crate::types::AnalysisOptions::default(),
    )
    .unwrap();
    assert!(
        report.score >= 80.0,
        "Expected score >= 80, got {:.2}",
        report.score
    );
}

#[test]
fn java_constructor_conforms() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r"
public class Person {
    private String name;
    private int age;

    public Person(String name, int age) {
        this.name = name;
        this.age = age;
    }

    public String getName() {
        return name;
    }
}
",
        "Person.java",
    );
}

#[test]
fn java_try_with_resources_conforms() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r#"
import java.io.BufferedReader;
import java.io.FileReader;

public class FileUtils {
    public String readFile(String path) {
        try (BufferedReader br = new BufferedReader(new FileReader(path))) {
            return br.readLine();
        } catch (Exception e) {
            return "error";
        }
    }
}
"#,
        "FileUtils.java",
    );
}

#[test]
fn java_do_while_conforms() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r"
public class Looper {
    public int countdown(int n) {
        do {
            n--;
        } while (n > 0);
        return n;
    }
}
",
        "Looper.java",
    );
}

#[test]
fn java_anonymous_inner_class_conforms() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r"
import java.util.Comparator;

public class Sorter {
    public void sort(java.util.List<String> items) {
        items.sort(new Comparator<String>() {
            public int compare(String a, String b) {
                return a.compareTo(b);
            }
        });
    }
}
",
        "Sorter.java",
    );
}

#[test]
fn java_nested_class_conforms() {
    let adapter = JavaAdapter;
    check_conformance(
        &adapter,
        r"
public class Outer {
    public void outerMethod() {
        int x = 1;
    }

    public static class Inner {
        public void innerMethod() {
            int y = 2;
        }
    }
}
",
        "Outer.java",
    );
}

#[test]
fn java_excessive_returns_flagged_when_enabled() {
    let source = r#"
public class HttpStatus {
    public String httpStatusMessage(int code) {
        if (code == 200) return "OK";
        if (code == 201) return "Created";
        if (code == 204) return "No Content";
        if (code == 301) return "Moved Permanently";
        if (code == 302) return "Found";
        if (code == 400) return "Bad Request";
        if (code == 401) return "Unauthorized";
        if (code == 403) return "Forbidden";
        if (code == 404) return "Not Found";
        if (code == 405) return "Method Not Allowed";
        if (code == 500) return "Internal Server Error";
        if (code == 502) return "Bad Gateway";
        if (code == 503) return "Service Unavailable";
        return "Unknown";
    }
}
"#;
    // Enable excessiveReturns flag (disabled by default)
    let mut overrides = HashMap::new();
    overrides.insert(
        "excessiveReturns".to_string(),
        crate::types::FlagConfig::Enabled(true),
    );
    let mut opts = crate::types::AnalysisOptions::default();
    opts.flag_overrides = Some(overrides);

    let report = crate::analyzer::analyze_source_str(source, "HttpStatus.java", &opts).unwrap();
    let method = report
        .classes
        .iter()
        .flat_map(|c| &c.methods)
        .find(|m| m.name == "httpStatusMessage")
        .expect("Expected httpStatusMessage method");

    let has_excessive_returns = method
        .flags
        .iter()
        .any(|f| f.flag_type == crate::types::FlagType::ExcessiveReturns);
    assert!(
        has_excessive_returns,
        "httpStatusMessage has 14 returns (threshold: 3 warn / 4 error) but excessiveReturns did not fire. \
         return_count={}, flags={:?}",
        method.metrics.structural.return_count,
        method.flags,
    );
}
