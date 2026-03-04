//! Conformance tests for language adapters.
//!
//! These tests validate that a `LanguageAdapter` implementation meets the
//! IR event contract. They run against every registered adapter with a set
//! of known-good source strings. When adding a new language, it automatically
//! gets tested by these conformance checks.

#[cfg(test)]
mod tests {
    use crate::ir::events::QualitasEvent;
    use crate::ir::language::LanguageAdapter;
    use crate::languages::rust::RustAdapter;
    use crate::languages::typescript::TypeScriptAdapter;

    /// Run all conformance checks for a given adapter + source pair.
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

    fn check_function_conformance(
        adapter_name: &str,
        func: &crate::ir::language::FunctionExtraction,
    ) {
        // Function names must not be empty
        assert!(
            !func.name.is_empty(),
            "[{adapter_name}] Function has empty name at byte {}-{}",
            func.byte_start,
            func.byte_end,
        );

        // Line numbers must be valid
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

        // NestingEnter/NestingExit must be balanced
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

        // NestedFunctionEnter/NestedFunctionExit must be balanced
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

    // ── TypeScript conformance ──────────────────────────────────────────────

    #[test]
    fn ts_clean_code_conforms() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r"
function add(a: number, b: number): number { return a + b; }
function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}
",
            "clean.ts",
        );
    }

    #[test]
    fn ts_complex_code_conforms() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r"
function processOrders(orders: any[], config: any, logger: any, db: any, cache: any, validator: any) {
  const results: any[] = [];
  for (const order of orders) {
    if (order.status === 'pending') {
      if (order.items && order.items.length > 0) {
        for (const item of order.items) {
          if (item.quantity > 0) {
            try {
              if (validator.isValid(item)) {
                results.push({ status: 'processed' });
              }
            } catch (err: any) {
              logger.error(err.message);
            }
          }
        }
      }
    }
  }
  return results;
}
",
            "complex.ts",
        );
    }

    #[test]
    fn ts_arrow_functions_conform() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r"
const add = (a: number, b: number) => a + b;
const process = (items: any[]) => {
  const result: any[] = [];
  for (const item of items) {
    if (item.active) { result.push(item); }
  }
  return result;
};
",
            "arrows.ts",
        );
    }

    #[test]
    fn ts_class_methods_conform() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r"
class Calculator {
  add(a: number, b: number) { return a + b; }
  subtract(a: number, b: number) { return a - b; }
  handler = (e: any) => { return e.target; };
}
",
            "class.ts",
        );
    }

    #[test]
    fn ts_object_literals_conform() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r"
const handlers = {
  onClick: (e: any) => { console.log(e); return e.target; },
  onHover: (e: any) => e,
  fetch: function(url: string) { return url; },
};
",
            "objects.ts",
        );
    }

    #[test]
    fn ts_export_default_conforms() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            "export default (a: number, b: number) => a + b;",
            "default.ts",
        );
    }

    #[test]
    fn ts_empty_source_conforms() {
        let adapter = TypeScriptAdapter;
        let extraction = TypeScriptAdapter.extract("", "empty.ts").unwrap();
        assert!(extraction.functions.is_empty());
        assert!(extraction.classes.is_empty());
        assert!(extraction.imports.is_empty());
        // Empty source should not crash
        let _ = adapter;
    }

    #[test]
    fn ts_syntax_error_does_not_panic() {
        let adapter = TypeScriptAdapter;
        // Invalid syntax — should not panic, may return empty or partial results
        let result = adapter.extract("function {{{", "bad.ts");
        assert!(result.is_ok()); // should not error, just warn on stderr
    }

    #[test]
    fn ts_nested_callbacks_have_balanced_nesting() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r"
function fetchAll(urls: string[]) {
  return urls.map((url) => {
    return fetch(url).then((res) => {
      return res.json();
    });
  });
}
",
            "callbacks.ts",
        );
    }

    // ── Rust conformance ────────────────────────────────────────────────────

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

    // ── Semantic correctness tests ────────────────────────────────────────
    //
    // These tests verify ACTUAL metric values produced by the adapters,
    // not just structural invariants.

    use crate::metrics::cognitive_flow::compute_cfc;

    /// Helper: extract the first function from a TS source and return its events.
    fn ts_first_fn_events(source: &str) -> Vec<QualitasEvent> {
        let adapter = TypeScriptAdapter;
        let extraction = adapter.extract(source, "test.ts").unwrap();
        assert!(
            !extraction.functions.is_empty(),
            "Expected at least one function in source",
        );
        extraction.functions.into_iter().next().unwrap().events
    }

    /// Helper: extract the first function from a TS source.
    fn ts_first_fn(source: &str) -> crate::ir::language::FunctionExtraction {
        let adapter = TypeScriptAdapter;
        let extraction = adapter.extract(source, "test.ts").unwrap();
        assert!(
            !extraction.functions.is_empty(),
            "Expected at least one function in source",
        );
        extraction.functions.into_iter().next().unwrap()
    }

    /// Helper: extract the first function from a Rust source and return its events.
    fn rs_first_fn_events(source: &str) -> Vec<QualitasEvent> {
        let adapter = RustAdapter;
        let extraction = adapter.extract(source, "test.rs").unwrap();
        // Functions may be top-level or inside an impl block
        if !extraction.functions.is_empty() {
            return extraction.functions.into_iter().next().unwrap().events;
        }
        // Fall back to first method of first class
        assert!(
            !extraction.classes.is_empty() && !extraction.classes[0].methods.is_empty(),
            "Expected at least one function or method in Rust source",
        );
        extraction
            .classes
            .into_iter()
            .next()
            .unwrap()
            .methods
            .into_iter()
            .next()
            .unwrap()
            .events
    }

    #[test]
    fn ts_simple_if_has_cfc_1() {
        let source = "function f(x: any) { if (x) {} }";
        let events = ts_first_fn_events(source);
        let cfc = compute_cfc(&events);
        assert_eq!(
            cfc.score, 1,
            "Simple if should have CFC score 1, got {}",
            cfc.score,
        );
    }

    #[test]
    fn ts_nested_if_has_cfc_at_least_3() {
        let source = r"
function f(x: any, y: any) {
    if (x) {
        if (y) {
            return 1;
        }
    }
}
";
        let events = ts_first_fn_events(source);
        let cfc = compute_cfc(&events);
        assert!(
            cfc.score >= 3,
            "Nested if should have CFC score >= 3, got {}",
            cfc.score,
        );
    }

    #[test]
    fn ts_function_with_5_params_flagged() {
        let source =
            "function f(a: number, b: number, c: number, d: number, e: number) { return a; }";
        let func = ts_first_fn(source);
        assert_eq!(
            func.param_count, 5,
            "Expected param_count=5, got {}",
            func.param_count,
        );
    }

    #[test]
    fn rs_match_with_3_arms_has_cfc() {
        let source = r#"
fn describe(value: Option<i32>) -> &'static str {
    match value {
        Some(n) if n > 0 => "positive",
        Some(_) => "other",
        None => "nothing",
    }
}
"#;
        let events = rs_first_fn_events(source);
        let cfc = compute_cfc(&events);
        assert!(
            cfc.score > 0,
            "Rust match with 3 arms should have CFC > 0, got {}",
            cfc.score,
        );
    }

    #[test]
    fn rs_for_loop_increments_cfc() {
        let source = r"
fn sum(items: &[i32]) -> i32 {
    let mut total = 0;
    for item in items {
        total += item;
    }
    total
}
";
        let events = rs_first_fn_events(source);
        let cfc = compute_cfc(&events);
        assert!(
            cfc.score >= 1,
            "Rust for loop should increment CFC to at least 1, got {}",
            cfc.score,
        );
        // Verify the event stream includes a ForOf control flow event
        let has_for = events.iter().any(|e| {
            matches!(
                e,
                QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                    kind: crate::ir::events::ControlFlowKind::ForOf,
                    ..
                })
            )
        });
        assert!(
            has_for,
            "Expected a ForOf control flow event for Rust for loop"
        );
    }
}
