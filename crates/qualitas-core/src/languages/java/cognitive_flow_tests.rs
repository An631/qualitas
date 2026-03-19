use crate::ir::events::QualitasEvent;
use crate::metrics::cognitive_flow::compute_cfc;

#[test]
fn java_if_else_has_cfc() {
    let source = r#"
public class Test {
    public String check(int x) {
        if (x > 0) {
            return "positive";
        } else if (x < 0) {
            return "negative";
        } else {
            return "zero";
        }
    }
}
"#;
    let events = super::java_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score > 0,
        "Java if/else should have CFC > 0, got {}",
        cfc.score,
    );
}

#[test]
fn java_for_loop_increments_cfc() {
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
    let events = super::java_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Java for should have CFC >= 1, got {}",
        cfc.score
    );
    let has_for = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::For,
                ..
            })
        )
    });
    assert!(
        has_for,
        "Expected a For control flow event for classic for loop"
    );
}

#[test]
fn java_enhanced_for_increments_cfc() {
    let source = r"
import java.util.List;

public class Test {
    public int sum(List<Integer> items) {
        int total = 0;
        for (int item : items) {
            total += item;
        }
        return total;
    }
}
";
    let events = super::java_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Java enhanced for should have CFC >= 1, got {}",
        cfc.score
    );
    let has_for_of = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::ForOf,
                ..
            })
        )
    });
    assert!(
        has_for_of,
        "Expected a ForOf control flow event for enhanced for"
    );
}

#[test]
fn java_while_increments_cfc() {
    let source = r"
public class Test {
    public int countdown(int n) {
        while (n > 0) {
            n--;
        }
        return n;
    }
}
";
    let events = super::java_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Java while should have CFC >= 1, got {}",
        cfc.score
    );
}

#[test]
fn java_do_while_increments_cfc() {
    let source = r"
public class Test {
    public int countdown(int n) {
        do {
            n--;
        } while (n > 0);
        return n;
    }
}
";
    let events = super::java_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Java do-while should have CFC >= 1, got {}",
        cfc.score
    );
    let has_do_while = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::DoWhile,
                ..
            })
        )
    });
    assert!(has_do_while, "Expected a DoWhile control flow event");
}

#[test]
fn java_switch_increments_cfc() {
    let source = r#"
public class Test {
    public String classify(int x) {
        switch (x) {
            case 1:
                return "one";
            case 2:
                return "two";
            default:
                return "other";
        }
    }
}
"#;
    let events = super::java_first_fn_events(source);
    let cfc = compute_cfc(&events);
    assert!(
        cfc.score >= 1,
        "Java switch should have CFC >= 1, got {}",
        cfc.score
    );
}

#[test]
fn java_try_catch_increments_cfc() {
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
    let events = super::java_first_fn_events(source);
    let has_catch = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::ControlFlow(crate::ir::events::ControlFlowEvent {
                kind: crate::ir::events::ControlFlowKind::Catch,
                ..
            })
        )
    });
    assert!(has_catch, "Expected a Catch control flow event");
}

#[test]
fn java_boolean_operators_increment_cfc() {
    let source = r"
public class Test {
    public boolean check(boolean a, boolean b, boolean c) {
        if (a && b || c) {
            return true;
        }
        return false;
    }
}
";
    let events = super::java_first_fn_events(source);
    let has_and = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::LogicOp(crate::ir::events::LogicOpEvent::And)
        )
    });
    let has_or = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::LogicOp(crate::ir::events::LogicOpEvent::Or)
        )
    });
    assert!(has_and, "Expected And logic op");
    assert!(has_or, "Expected Or logic op");
}

#[test]
fn java_ternary_has_logic_op() {
    let source = r"
public class Test {
    public int pick(boolean flag, int a, int b) {
        return flag ? a : b;
    }
}
";
    let events = super::java_first_fn_events(source);
    let has_ternary = events.iter().any(|e| {
        matches!(
            e,
            QualitasEvent::LogicOp(crate::ir::events::LogicOpEvent::Ternary)
        )
    });
    assert!(has_ternary, "Expected Ternary logic op for ? : expression");
}

#[test]
fn java_recursive_call_detected() {
    let source = r"
public class Test {
    public int factorial(int n) {
        if (n <= 1) return 1;
        return n * factorial(n - 1);
    }
}
";
    let events = super::java_first_fn_events(source);
    let has_recursive = events
        .iter()
        .any(|e| matches!(e, QualitasEvent::RecursiveCall));
    assert!(has_recursive, "Expected RecursiveCall event for factorial");
}

#[test]
fn java_labeled_break_emits_labeled_flow() {
    let source = r"
public class Test {
    public void search(int[][] matrix, int target) {
        outer:
        for (int[] row : matrix) {
            for (int val : row) {
                if (val == target) {
                    break outer;
                }
            }
        }
    }
}
";
    let events = super::java_first_fn_events(source);
    let label_count = events
        .iter()
        .filter(|e| matches!(e, QualitasEvent::LabeledFlow))
        .count();
    assert!(
        label_count >= 2,
        "Expected at least 2 LabeledFlow events (labeled statement + break label), got {label_count}"
    );
}

#[test]
fn java_anonymous_class_cfc_isolated() {
    // CFC from control flow inside anonymous class should NOT inflate parent's CFC
    // because the anonymous class body is wrapped in NestedFunctionEnter/Exit
    let source = r"
import java.util.Comparator;

public class Test {
    public void sort(java.util.List<String> items) {
        items.sort(new Comparator<String>() {
            public int compare(String a, String b) {
                if (a.length() > b.length()) {
                    return 1;
                }
                return a.compareTo(b);
            }
        });
    }
}
";
    let adapter = super::JavaAdapter;
    let extraction =
        crate::ir::language::LanguageAdapter::extract(&adapter, source, "Test.java").unwrap();
    let test_class = extraction
        .classes
        .iter()
        .find(|c| c.name == "Test")
        .unwrap();
    let sort_events = &test_class.methods[0].events;
    // The if inside the anonymous class is between NestedFunctionEnter/Exit
    let has_nested_enter = sort_events
        .iter()
        .any(|e| matches!(e, QualitasEvent::NestedFunctionEnter));
    assert!(
        has_nested_enter,
        "Expected NestedFunctionEnter for anonymous class body"
    );
}

#[test]
fn java_nested_class_methods_have_own_cfc() {
    let source = r"
public class Outer {
    public void outerMethod() {
        int x = 1;
    }

    public static class Inner {
        public void innerMethod() {
            if (true) {
                int y = 2;
            }
        }
    }
}
";
    let adapter = super::JavaAdapter;
    let extraction =
        crate::ir::language::LanguageAdapter::extract(&adapter, source, "Outer.java").unwrap();

    // Should have 2 classes: Outer and Outer.Inner
    assert!(
        extraction.classes.len() >= 2,
        "Expected at least 2 classes (Outer + Outer.Inner), got {}",
        extraction.classes.len(),
    );

    let inner = extraction
        .classes
        .iter()
        .find(|c| c.name.contains("Inner"))
        .unwrap();
    let inner_method = &inner.methods[0];
    let inner_cfc = compute_cfc(&inner_method.events);
    assert!(
        inner_cfc.score >= 1,
        "Inner class method with if should have CFC >= 1, got {}",
        inner_cfc.score,
    );
}
