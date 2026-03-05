pub mod ts_adapter;
pub use ts_adapter::TypeScriptAdapter;

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

/// Helper: extract the first function from a TS source and return its events.
#[cfg(test)]
pub(crate) fn ts_first_fn_events(source: &str) -> Vec<crate::ir::events::QualitasEvent> {
    use crate::ir::language::LanguageAdapter;
    let adapter = TypeScriptAdapter;
    let extraction = adapter.extract(source, "test.ts").unwrap();
    assert!(
        !extraction.functions.is_empty(),
        "Expected at least one function in source",
    );
    extraction.functions.into_iter().next().unwrap().events
}

/// Helper: extract the first function from a TS source.
#[cfg(test)]
pub(crate) fn ts_first_fn(source: &str) -> crate::ir::language::FunctionExtraction {
    use crate::ir::language::LanguageAdapter;
    let adapter = TypeScriptAdapter;
    let extraction = adapter.extract(source, "test.ts").unwrap();
    assert!(
        !extraction.functions.is_empty(),
        "Expected at least one function in source",
    );
    extraction.functions.into_iter().next().unwrap()
}
