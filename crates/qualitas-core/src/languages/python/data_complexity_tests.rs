use crate::metrics::data_complexity::compute_dci;

#[test]
fn py_empty_function_has_zero_dci() {
    let source = r"
def empty():
    pass
";
    let events = super::py_first_fn_events(source);
    let dci = compute_dci(&events);
    assert_eq!(
        dci.difficulty, 0.0,
        "Empty Python function should have zero DCI difficulty, got {}",
        dci.difficulty,
    );
}

#[test]
fn py_simple_addition_has_operators() {
    let source = r"
def add(a, b):
    return a + b
";
    let events = super::py_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 1,
        "Python addition should have at least 1 operator, got {}",
        dci.halstead.total_operators,
    );
    assert!(
        dci.halstead.total_operands >= 2,
        "Python addition should have at least 2 operands, got {}",
        dci.halstead.total_operands,
    );
}

#[test]
fn py_augmented_assignment_has_operator() {
    let source = r"
def accumulate(items):
    total = 0
    for item in items:
        total += item
    return total
";
    let events = super::py_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 2,
        "Augmented assignment should count as operator, got {}",
        dci.halstead.total_operators,
    );
}
