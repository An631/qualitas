pub mod py_adapter;
pub use py_adapter::PythonAdapter;

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

/// Helper: extract the first function from a Python source and return its events.
#[cfg(test)]
pub(crate) fn py_first_fn_events(source: &str) -> Vec<crate::ir::events::QualitasEvent> {
    use crate::ir::language::LanguageAdapter;
    let adapter = PythonAdapter;
    let extraction = adapter.extract(source, "test.py").unwrap();
    if !extraction.functions.is_empty() {
        return extraction.functions.into_iter().next().unwrap().events;
    }
    assert!(
        !extraction.classes.is_empty() && !extraction.classes[0].methods.is_empty(),
        "Expected at least one function or method in Python source",
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
