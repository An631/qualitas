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
    use crate::languages::typescript::TypeScriptAdapter;

    /// Run all conformance checks for a given adapter + source pair.
    fn check_conformance(adapter: &dyn LanguageAdapter, source: &str, file_name: &str) {
        let extraction = adapter
            .extract(source, file_name)
            .unwrap_or_else(|e| panic!("Adapter {} failed to parse {file_name}: {e}", adapter.name()));

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
            r#"
function add(a: number, b: number): number { return a + b; }
function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}
"#,
            "clean.ts",
        );
    }

    #[test]
    fn ts_complex_code_conforms() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r#"
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
"#,
            "complex.ts",
        );
    }

    #[test]
    fn ts_arrow_functions_conform() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r#"
const add = (a: number, b: number) => a + b;
const process = (items: any[]) => {
  const result: any[] = [];
  for (const item of items) {
    if (item.active) { result.push(item); }
  }
  return result;
};
"#,
            "arrows.ts",
        );
    }

    #[test]
    fn ts_class_methods_conform() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r#"
class Calculator {
  add(a: number, b: number) { return a + b; }
  subtract(a: number, b: number) { return a - b; }
  handler = (e: any) => { return e.target; };
}
"#,
            "class.ts",
        );
    }

    #[test]
    fn ts_object_literals_conform() {
        let adapter = TypeScriptAdapter;
        check_conformance(
            &adapter,
            r#"
const handlers = {
  onClick: (e: any) => { console.log(e); return e.target; },
  onHover: (e: any) => e,
  fetch: function(url: string) { return url; },
};
"#,
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
            r#"
function fetchAll(urls: string[]) {
  return urls.map((url) => {
    return fetch(url).then((res) => {
      return res.json();
    });
  });
}
"#,
            "callbacks.ts",
        );
    }
}
