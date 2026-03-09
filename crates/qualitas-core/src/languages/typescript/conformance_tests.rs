use super::TypeScriptAdapter;
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
    let _ = adapter;
}

#[test]
fn ts_clean_function_scores_high() {
    let report = crate::analyzer::analyze_source_str(
        "function add(a: number, b: number): number { return a + b; }",
        "clean.ts",
        &crate::types::AnalysisOptions::default(),
    )
    .unwrap();
    assert!(
        report.score >= 80.0,
        "Expected score >= 80, got {:.2}",
        report.score
    );
    assert_eq!(report.grade, crate::types::Grade::A);
}

#[test]
fn ts_complex_function_scores_low() {
    let source = r"
function processOrders(orders: any[], config: any, logger: any, db: any, cache: any, validator: any) {
    const results: any[] = [];
    for (const order of orders) {
        if (order.status === 'pending') {
            if (order.items && order.items.length > 0) {
                for (const item of order.items) {
                    if (item.quantity > 0) {
                        try {
                            if (validator.isValid(item)) {
                                if (config.dryRun || config.verbose && logger.level === 'debug') {
                                    logger.info('processing');
                                }
                                const price = item.price * item.quantity;
                                if (price > config.maxPrice) {
                                    results.push({ status: 'skipped', reason: 'too expensive' });
                                } else {
                                    results.push({ status: 'processed', price: price });
                                }
                            }
                        } catch (err: any) {
                            if (err.code === 'NETWORK') {
                                logger.error(err.message);
                                cache.invalidate(order.id);
                            } else {
                                db.log(err);
                            }
                        }
                    }
                }
            }
        }
    }
    return results;
}
";
    let report = crate::analyzer::analyze_source_str(
        source,
        "complex.ts",
        &crate::types::AnalysisOptions::default(),
    )
    .unwrap();
    assert!(
        report.score < 65.0,
        "Expected score < 65, got {:.2}",
        report.score
    );
}

#[test]
fn ts_class_aggregates_methods() {
    let source = r"
class Calculator {
    add(a: number, b: number) { return a + b; }
    subtract(a: number, b: number) { return a - b; }
}
";
    let report = crate::analyzer::analyze_source_str(
        source,
        "class.ts",
        &crate::types::AnalysisOptions::default(),
    )
    .unwrap();
    assert_eq!(report.class_count, 1);
    assert_eq!(report.function_count, 2);
    assert!(
        report.functions.is_empty(),
        "Methods should be in classes, not top-level"
    );
    assert_eq!(report.classes[0].methods.len(), 2);
}

#[test]
fn ts_syntax_error_does_not_panic() {
    let adapter = TypeScriptAdapter;
    let result = adapter.extract("function {{{", "bad.ts");
    assert!(result.is_ok());
}

#[test]
fn ts_multibyte_utf8_string_literals_do_not_panic() {
    let adapter = TypeScriptAdapter;
    // String with 30 ASCII bytes + a 3-byte UTF-8 char puts byte 32 mid-char
    let source = "function f() {\n  const a = \"==============================\u{2550}\u{2550}\";\n  const b = \"\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\";\n  return a + b;\n}\n";
    check_conformance(&adapter, source, "utf8.ts");

    // Also verify full analysis pipeline doesn't panic
    let source2 = "function g() { return \"==============================\u{2550}\u{2550}\"; }";
    let report = crate::analyzer::analyze_source_str(
        source2,
        "utf8.ts",
        &crate::types::AnalysisOptions::default(),
    )
    .unwrap();
    assert!(report.score > 0.0);
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
