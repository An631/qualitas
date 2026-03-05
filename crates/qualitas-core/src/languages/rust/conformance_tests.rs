use super::RustAdapter;
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
fn rs_clean_code_conforms() {
    let adapter = RustAdapter;
    check_conformance(
        &adapter,
        r"
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn capitalize(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap().to_uppercase().to_string();
    first + chars.as_str()
}
",
        "clean.rs",
    );
}

#[test]
fn rs_complex_code_conforms() {
    let adapter = RustAdapter;
    check_conformance(
        &adapter,
        r"
fn process_orders(
    orders: &[Order],
    config: &Config,
    logger: &Logger,
    db: &Database,
    cache: &Cache,
    validator: &Validator,
) -> Vec<Result<(), Error>> {
    let mut results = Vec::new();
    for order in orders {
        if order.status == Status::Pending {
            if !order.items.is_empty() {
                for item in &order.items {
                    if item.quantity > 0 {
                        match validator.validate(item) {
                            Ok(_) => {
                                results.push(Ok(()));
                            }
                            Err(e) => {
                                logger.error(&e.to_string());
                                results.push(Err(e));
                            }
                        }
                    }
                }
            }
        }
    }
    results
}
",
        "complex.rs",
    );
}

#[test]
fn rs_impl_methods_conform() {
    let adapter = RustAdapter;
    check_conformance(
        &adapter,
        r"
struct Calculator;

impl Calculator {
    fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }

    fn subtract(&self, a: i32, b: i32) -> i32 {
        a - b
    }
}
",
        "impl.rs",
    );
}

#[test]
fn rs_closures_conform() {
    let adapter = RustAdapter;
    check_conformance(
        &adapter,
        r"
fn process(items: Vec<i32>) -> Vec<i32> {
    items.iter()
        .filter(|x| **x > 0)
        .map(|x| x * 2)
        .collect()
}
",
        "closures.rs",
    );
}

#[test]
fn rs_match_conforms() {
    let adapter = RustAdapter;
    check_conformance(
        &adapter,
        r#"
fn describe(value: Option<i32>) -> &'static str {
    match value {
        Some(n) if n > 0 => "positive",
        Some(0) => "zero",
        Some(_) => "negative",
        None => "nothing",
    }
}
"#,
        "match.rs",
    );
}

#[test]
fn rs_empty_source_conforms() {
    let adapter = RustAdapter;
    let extraction = adapter.extract("", "empty.rs").unwrap();
    assert!(extraction.functions.is_empty());
    assert!(extraction.classes.is_empty());
}

#[test]
fn rs_syntax_error_returns_err() {
    let adapter = RustAdapter;
    let result = adapter.extract("fn {{{", "bad.rs");
    assert!(result.is_err());
}

#[test]
fn rs_nested_closures_have_balanced_nesting() {
    let adapter = RustAdapter;
    check_conformance(
        &adapter,
        r"
fn transform(data: Vec<Vec<i32>>) -> Vec<Vec<i32>> {
    data.into_iter()
        .map(|inner| {
            inner.into_iter()
                .filter(|x| *x > 0)
                .collect()
        })
        .collect()
}
",
        "nested_closures.rs",
    );
}

#[test]
fn rs_async_code_conforms() {
    let adapter = RustAdapter;
    check_conformance(
        &adapter,
        r"
async fn fetch_data(url: &str) -> Result<String, Error> {
    let response = client.get(url).await?;
    let body = response.text().await?;
    Ok(body)
}
",
        "async.rs",
    );
}
