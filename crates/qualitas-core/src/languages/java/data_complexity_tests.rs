use crate::metrics::data_complexity::compute_dci;

#[test]
fn java_empty_method_has_zero_dci() {
    let source = r"
public class Test {
    public void empty() {
    }
}
";
    let events = super::java_first_fn_events(source);
    let dci = compute_dci(&events);
    assert_eq!(
        dci.difficulty, 0.0,
        "Empty Java method should have zero DCI difficulty, got {}",
        dci.difficulty,
    );
}

#[test]
fn java_simple_addition_has_operators() {
    let source = r"
public class Test {
    public int add(int a, int b) {
        return a + b;
    }
}
";
    let events = super::java_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 1,
        "Java addition should have at least 1 operator, got {}",
        dci.halstead.total_operators,
    );
    assert!(
        dci.halstead.total_operands >= 2,
        "Java addition should have at least 2 operands, got {}",
        dci.halstead.total_operands,
    );
}

#[test]
fn java_assignment_has_operator() {
    let source = r"
public class Test {
    public void assign() {
        int x = 5;
    }
}
";
    let events = super::java_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 1,
        "Assignment should count as operator, got {}",
        dci.halstead.total_operators,
    );
}

#[test]
fn java_compound_assignment_has_operator() {
    let source = r"
public class Test {
    public int accumulate(int[] items) {
        int total = 0;
        for (int item : items) {
            total += item;
        }
        return total;
    }
}
";
    let events = super::java_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 2,
        "Compound assignment should count as operator, got {}",
        dci.halstead.total_operators,
    );
}

#[test]
fn java_anonymous_class_operators_emitted() {
    // DCI continues through NestedFunctionEnter/Exit boundaries
    let source = r"
import java.util.Comparator;

public class Test {
    public void sort(java.util.List<String> items) {
        items.sort(new Comparator<String>() {
            public int compare(String a, String b) {
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
    let dci = compute_dci(sort_events);
    assert!(
        dci.halstead.total_operators >= 1,
        "Anonymous class should contribute operators to DCI (via new operator + inner body), got {}",
        dci.halstead.total_operators,
    );
}
