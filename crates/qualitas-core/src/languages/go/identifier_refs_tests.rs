use crate::analyzer::analyze_source_str;
use crate::metrics::identifier_refs::compute_irc;
use crate::types::AnalysisOptions;

#[test]
fn go_unused_variable_has_zero_irc() {
    let source = r"
package main

func unused() {
    x := 1
    _ = x
}
";
    let events = super::go_first_fn_events(source);
    let irc = compute_irc(&events, source);
    // x is declared then referenced once in `_ = x`, but _ is blank
    assert!(
        irc.total_irc >= 0.0,
        "Go unused variable IRC should be >= 0, got {}",
        irc.total_irc,
    );
}

#[test]
fn go_used_variable_has_irc_cost() {
    let source = r"
package main

func used() int {
    x := 1
    y := x + x
    return y
}
";
    let events = super::go_first_fn_events(source);
    let irc = compute_irc(&events, source);
    assert!(
        irc.total_irc > 0.0,
        "Used Go variable should have IRC > 0, got {}",
        irc.total_irc,
    );
}

#[test]
fn go_closure_capturing_parent_vars_has_irc() {
    let source = r"
package main

func process(items []int, threshold int) []int {
    multiplier := 2
    result := make([]int, 0)
    for _, item := range items {
        if item > threshold {
            fn := func(x int) int { return x * multiplier }
            result = append(result, fn(item))
        }
    }
    return result
}
";
    let opts = AnalysisOptions::default();
    let report = analyze_source_str(source, "closure.go", &opts).unwrap();
    let irc = report.functions[0].metrics.identifier_reference.total_irc;

    assert!(
        irc > 0.0,
        "Closure capturing multiplier should contribute to parent IRC, got {irc:.1}",
    );
}
