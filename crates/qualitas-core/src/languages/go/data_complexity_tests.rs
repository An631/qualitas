use crate::metrics::data_complexity::compute_dci;

#[test]
fn go_empty_function_has_zero_dci() {
    let source = r"
package main

func empty() {
}
";
    let events = super::go_first_fn_events(source);
    let dci = compute_dci(&events);
    assert_eq!(
        dci.difficulty, 0.0,
        "Empty Go function should have zero DCI difficulty, got {}",
        dci.difficulty,
    );
}

#[test]
fn go_simple_addition_has_operators() {
    let source = r"
package main

func add(a, b int) int {
    return a + b
}
";
    let events = super::go_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 1,
        "Go addition should have at least 1 operator, got {}",
        dci.halstead.total_operators,
    );
    assert!(
        dci.halstead.total_operands >= 2,
        "Go addition should have at least 2 operands, got {}",
        dci.halstead.total_operands,
    );
}

#[test]
fn go_short_var_decl_has_operator() {
    let source = r"
package main

func assign() {
    x := 5
    _ = x
}
";
    let events = super::go_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 1,
        "Short var decl := should count as operator, got {}",
        dci.halstead.total_operators,
    );
}

#[test]
fn go_augmented_assignment_has_operator() {
    let source = r"
package main

func accumulate(items []int) int {
    total := 0
    for _, item := range items {
        total += item
    }
    return total
}
";
    let events = super::go_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 2,
        "Augmented assignment should count as operator, got {}",
        dci.halstead.total_operators,
    );
}
