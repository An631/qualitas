use crate::metrics::data_complexity::compute_dci;

#[test]
fn rs_empty_function_has_zero_dci() {
    let source = r"
fn empty() {}
";
    let events = super::rs_first_fn_events(source);
    let dci = compute_dci(&events);
    assert_eq!(
        dci.difficulty, 0.0,
        "Empty Rust function should have zero DCI difficulty, got {}",
        dci.difficulty,
    );
}

#[test]
fn rs_simple_addition_has_operators() {
    let source = r"
fn add(a: i32, b: i32) -> i32 {
    a + b
}
";
    let events = super::rs_first_fn_events(source);
    let dci = compute_dci(&events);
    assert!(
        dci.halstead.total_operators >= 1,
        "Rust addition should have at least 1 operator, got {}",
        dci.halstead.total_operators,
    );
    assert!(
        dci.halstead.total_operands >= 2,
        "Rust addition should have at least 2 operands, got {}",
        dci.halstead.total_operands,
    );
}
