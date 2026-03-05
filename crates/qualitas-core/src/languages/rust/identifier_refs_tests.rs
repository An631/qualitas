use crate::metrics::identifier_refs::compute_irc;

#[test]
fn rs_unused_variable_has_zero_irc() {
    let source = r"
fn unused() {
    let x = 1;
}
";
    let events = super::rs_first_fn_events(source);
    let irc = compute_irc(&events, source);
    assert_eq!(
        irc.total_irc, 0.0,
        "Unused Rust variable should have zero IRC, got {}",
        irc.total_irc,
    );
}

#[test]
fn rs_used_variable_has_irc_cost() {
    let source = r"
fn used() -> i32 {
    let x = 1;
    let y = x + x;
    y
}
";
    let events = super::rs_first_fn_events(source);
    let irc = compute_irc(&events, source);
    assert!(
        irc.total_irc > 0.0,
        "Used Rust variable should have IRC > 0, got {}",
        irc.total_irc,
    );
}
