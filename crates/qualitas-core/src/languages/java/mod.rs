pub mod java_adapter;
pub use java_adapter::JavaAdapter;

#[cfg(test)]
mod cognitive_flow_tests;
#[cfg(test)]
mod conformance_tests;
#[cfg(test)]
mod data_complexity_tests;
#[cfg(test)]
mod dependency_coupling_tests;
#[cfg(test)]
mod identifier_refs_tests;
#[cfg(test)]
mod structural_tests;

/// Helper: extract the first method from a Java source and return its events.
#[cfg(test)]
pub(crate) fn java_first_fn_events(source: &str) -> Vec<crate::ir::events::QualitasEvent> {
    use crate::ir::language::LanguageAdapter;
    let adapter = JavaAdapter;
    let extraction = adapter.extract(source, "Test.java").unwrap();
    assert!(
        !extraction.classes.is_empty() && !extraction.classes[0].methods.is_empty(),
        "Expected at least one class with methods in Java source",
    );
    extraction
        .classes
        .into_iter()
        .next()
        .unwrap()
        .methods
        .into_iter()
        .next()
        .unwrap()
        .events
}
