pub mod rs_adapter;
pub use rs_adapter::RustAdapter;

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

/// Helper: extract the first function from a Rust source and return its events.
#[cfg(test)]
pub(crate) fn rs_first_fn_events(source: &str) -> Vec<crate::ir::events::QualitasEvent> {
    use crate::ir::language::LanguageAdapter;
    let adapter = RustAdapter;
    let extraction = adapter.extract(source, "test.rs").unwrap();
    // Functions may be top-level or inside an impl block
    if !extraction.functions.is_empty() {
        return extraction.functions.into_iter().next().unwrap().events;
    }
    // Fall back to first method of first class
    assert!(
        !extraction.classes.is_empty() && !extraction.classes[0].methods.is_empty(),
        "Expected at least one function or method in Rust source",
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
