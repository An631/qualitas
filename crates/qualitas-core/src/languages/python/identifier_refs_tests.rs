use crate::analyzer::analyze_source_str;
use crate::metrics::identifier_refs::compute_irc;
use crate::types::AnalysisOptions;

#[test]
fn py_unused_variable_has_zero_irc() {
    let source = r"
def unused():
    x = 1
";
    let events = super::py_first_fn_events(source);
    let irc = compute_irc(&events, source);
    assert_eq!(
        irc.total_irc, 0.0,
        "Unused Python variable should have zero IRC, got {}",
        irc.total_irc,
    );
}

#[test]
fn py_used_variable_has_irc_cost() {
    let source = r"
def used():
    x = 1
    y = x + x
    return y
";
    let events = super::py_first_fn_events(source);
    let irc = compute_irc(&events, source);
    assert!(
        irc.total_irc > 0.0,
        "Used Python variable should have IRC > 0, got {}",
        irc.total_irc,
    );
}

#[test]
fn py_lambda_capturing_parent_vars_has_irc() {
    let source = r"
def process(items, threshold):
    multiplier = 2
    filtered = list(filter(lambda x: x > threshold, items))
    result = list(map(lambda x: x * multiplier, filtered))
    return result
";
    let opts = AnalysisOptions::default();
    let report = analyze_source_str(source, "lambda.py", &opts).unwrap();
    let irc = report.functions[0].metrics.identifier_reference.total_irc;

    assert!(
        irc > 0.0,
        "Lambdas capturing threshold/multiplier should contribute to parent IRC, got {irc:.1}",
    );
}
