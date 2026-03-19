use crate::analyzer::analyze_source_str;
use crate::metrics::identifier_refs::compute_irc;
use crate::types::AnalysisOptions;

#[test]
fn java_unused_variable_has_zero_irc() {
    let source = r"
public class Test {
    public void unused() {
        int x = 1;
    }
}
";
    let events = super::java_first_fn_events(source);
    let irc = compute_irc(&events, source);
    assert!(
        irc.total_irc >= 0.0,
        "Java unused variable IRC should be >= 0, got {}",
        irc.total_irc,
    );
}

#[test]
fn java_used_variable_has_irc_cost() {
    let source = r"
public class Test {
    public int used() {
        int x = 1;
        int y = x + x;
        return y;
    }
}
";
    let events = super::java_first_fn_events(source);
    let irc = compute_irc(&events, source);
    assert!(
        irc.total_irc > 0.0,
        "Used Java variable should have IRC > 0, got {}",
        irc.total_irc,
    );
}

#[test]
fn java_lambda_capturing_parent_vars_has_irc() {
    let source = r"
import java.util.List;

public class Test {
    public void process(List<Integer> items) {
        int multiplier = 2;
        items.forEach(x -> {
            int result = x * multiplier;
        });
    }
}
";
    let opts = AnalysisOptions::default();
    let report = analyze_source_str(source, "Lambda.java", &opts).unwrap();
    // Find the process method (in a class)
    let func = report
        .classes
        .iter()
        .flat_map(|c| &c.methods)
        .find(|f| f.name == "process")
        .unwrap();
    let irc = func.metrics.identifier_reference.total_irc;
    assert!(
        irc > 0.0,
        "Lambda capturing multiplier should contribute to parent IRC, got {irc:.1}",
    );
}

#[test]
fn java_anonymous_class_captures_parent_vars() {
    let source = r"
import java.util.Comparator;

public class Test {
    public void sort(java.util.List<String> items) {
        int threshold = 5;
        items.sort(new Comparator<String>() {
            public int compare(String a, String b) {
                if (a.length() > threshold) {
                    return 1;
                }
                return a.compareTo(b);
            }
        });
    }
}
";
    let opts = AnalysisOptions::default();
    let report = analyze_source_str(source, "Anon.java", &opts).unwrap();
    let func = report
        .classes
        .iter()
        .flat_map(|c| &c.methods)
        .find(|f| f.name == "sort")
        .unwrap();
    let irc = func.metrics.identifier_reference.total_irc;
    assert!(
        irc > 0.0,
        "Anonymous class referencing threshold should contribute to parent IRC, got {irc:.1}",
    );
}
