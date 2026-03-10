use super::PythonAdapter;
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
fn py_clean_code_conforms() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r#"
def add(a, b):
    return a + b

def capitalize(s):
    if not s:
        return ""
    return s[0].upper() + s[1:]
"#,
        "clean.py",
    );
}

#[test]
fn py_complex_code_conforms() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r#"
def process_orders(orders, config, logger, db, cache, validator):
    results = []
    for order in orders:
        if order.status == "pending":
            if order.items:
                for item in order.items:
                    if item.quantity > 0:
                        try:
                            validator.validate(item)
                            results.append(True)
                        except Exception as e:
                            logger.error(str(e))
                            results.append(False)
    return results
"#,
        "complex.py",
    );
}

#[test]
fn py_class_methods_conform() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r"
class Calculator:
    def add(self, a, b):
        return a + b

    def subtract(self, a, b):
        return a - b
",
        "class.py",
    );
}

#[test]
fn py_lambdas_conform() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r"
def process(items):
    filtered = list(filter(lambda x: x > 0, items))
    doubled = list(map(lambda x: x * 2, filtered))
    return doubled
",
        "lambdas.py",
    );
}

#[test]
fn py_comprehensions_conform() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r"
def transform(data):
    evens = [x for x in data if x % 2 == 0]
    squares = {x: x**2 for x in evens}
    return squares
",
        "comprehensions.py",
    );
}

#[test]
fn py_empty_source_conforms() {
    let adapter = PythonAdapter;
    let extraction = adapter.extract("", "empty.py").unwrap();
    assert!(extraction.functions.is_empty());
    assert!(extraction.classes.is_empty());
}

#[test]
fn py_syntax_error_returns_err() {
    let adapter = PythonAdapter;
    let result = adapter.extract("def (((:", "bad.py");
    assert!(result.is_err());
}

#[test]
fn py_nested_functions_have_balanced_nesting() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r"
def outer():
    def inner():
        return 42
    return inner()
",
        "nested_fns.py",
    );
}

#[test]
fn py_async_code_conforms() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r"
async def fetch_data(url):
    response = await client.get(url)
    data = await response.json()
    return data
",
        "async.py",
    );
}

#[test]
fn py_with_statement_conforms() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r"
def read_file(path):
    with open(path) as f:
        return f.read()
",
        "with.py",
    );
}

#[test]
fn py_try_except_conforms() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r"
def safe_divide(a, b):
    try:
        result = a / b
    except ZeroDivisionError:
        result = 0
    except TypeError as e:
        raise ValueError(str(e))
    finally:
        pass
    return result
",
        "try.py",
    );
}

#[test]
fn py_clean_function_scores_high() {
    let source = r"
def add(a, b):
    return a + b
";
    let report = crate::analyzer::analyze_source_str(
        source,
        "clean.py",
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
fn py_decorated_function_conforms() {
    let adapter = PythonAdapter;
    check_conformance(
        &adapter,
        r#"
import functools

def my_decorator(func):
    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        return func(*args, **kwargs)
    return wrapper

@my_decorator
def greet(name):
    return f"Hello, {name}"
"#,
        "decorated.py",
    );
}

#[test]
fn py_self_param_stripped_from_method_count() {
    let adapter = PythonAdapter;
    let extraction = adapter
        .extract(
            r"
class Foo:
    def bar(self, x, y):
        return x + y
",
            "method.py",
        )
        .unwrap();
    let method = &extraction.classes[0].methods[0];
    assert_eq!(
        method.param_count, 2,
        "Method param_count should exclude self, got {}",
        method.param_count,
    );
}
