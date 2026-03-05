/// Dependency Coupling (DC) metric
///
/// Measures how many external dependencies and distinct APIs a file/function touches.
use std::collections::HashSet;

use crate::ir::language::ImportRecord as IrImportRecordType;
use crate::types::DependencyCouplingResult;

/// Build a file-level DC result from import records.
fn build_file_dc_result(imports: &[IrImportRecordType]) -> DependencyCouplingResult {
    let (external_packages, internal_modules) = classify_imports(imports);
    let import_count = imports.len() as u32;
    build_dc_result(external_packages, internal_modules, import_count, 0)
}

/// Analyze file-level import dependencies (from IR import records).
pub fn analyze_file_dependencies_ir(imports: &[IrImportRecordType]) -> DependencyCouplingResult {
    build_file_dc_result(imports)
}

/// Build a DC result from classified import sets and an API call count.
fn build_dc_result(
    external_packages: HashSet<String>,
    internal_modules: HashSet<String>,
    import_count: u32,
    distinct_api_calls: u32,
) -> DependencyCouplingResult {
    let distinct_sources = (external_packages.len() + internal_modules.len()) as u32;
    let external_ratio = compute_external_ratio(external_packages.len(), import_count);
    let raw_score = compute_dc_raw(import_count, external_ratio, distinct_api_calls);

    DependencyCouplingResult {
        import_count,
        distinct_sources,
        external_ratio,
        external_packages: external_packages.into_iter().collect(),
        internal_modules: internal_modules.into_iter().collect(),
        distinct_api_calls,
        closure_captures: 0,
        raw_score,
    }
}

fn root_package_name(source: &str) -> String {
    if source.starts_with('@') {
        let parts: Vec<&str> = source.splitn(3, '/').collect();
        if parts.len() >= 2 {
            return format!("{}/{}", parts[0], parts[1]);
        }
    }
    source.split('/').next().unwrap_or(source).to_string()
}

pub fn compute_dc_raw(import_count: u32, external_ratio: f64, distinct_api_calls: u32) -> f64 {
    use crate::constants::{
        DC_API_CALLS_WEIGHT, DC_EXTERNAL_RATIO_WEIGHT, DC_IMPORT_WEIGHT, NORM_DC_API_CALLS,
        NORM_DC_IMPORTS,
    };
    DC_IMPORT_WEIGHT * (f64::from(import_count) / NORM_DC_IMPORTS)
        + DC_EXTERNAL_RATIO_WEIGHT * external_ratio
        + DC_API_CALLS_WEIGHT * (f64::from(distinct_api_calls) / NORM_DC_API_CALLS)
}

// ─── Event-based DC computation ─────────────────────────────────────────────

use crate::ir::events::QualitasEvent;

/// Collect distinct API call keys from events.
fn collect_api_calls(events: &[QualitasEvent]) -> HashSet<String> {
    let mut api_calls = HashSet::new();
    for event in events {
        if let QualitasEvent::ApiCall(call) = event {
            api_calls.insert(format!("{}.{}", call.object, call.method));
        }
    }
    api_calls
}

/// Classify imports into external packages and internal modules.
fn classify_imports(imports: &[IrImportRecordType]) -> (HashSet<String>, HashSet<String>) {
    let mut external_packages = HashSet::new();
    let mut internal_modules = HashSet::new();
    for import in imports {
        if import.is_external {
            external_packages.insert(root_package_name(&import.source));
        } else {
            internal_modules.insert(import.source.clone());
        }
    }
    (external_packages, internal_modules)
}

/// Compute the external ratio given external count and total import count.
fn compute_external_ratio(external_count: usize, import_count: u32) -> f64 {
    if import_count > 0 {
        external_count as f64 / f64::from(import_count)
    } else {
        0.0
    }
}

/// Collect names referenced in a function's events (identifiers + API call objects).
fn collect_referenced_names(events: &[QualitasEvent]) -> HashSet<String> {
    events
        .iter()
        .filter_map(|e| match e {
            QualitasEvent::IdentReference(id) => Some(id.name.clone()),
            QualitasEvent::ApiCall(call) => Some(call.object.clone()),
            _ => None,
        })
        .collect()
}

/// Filter imports to only those whose bindings are actually referenced in the function.
fn filter_used_imports<'a>(
    imports: &'a [IrImportRecordType],
    referenced: &HashSet<String>,
) -> Vec<&'a IrImportRecordType> {
    imports
        .iter()
        .filter(|imp| imp.names.iter().any(|n| referenced.contains(n)))
        .collect()
}

/// Compute function-level DC from a stream of IR events (language-agnostic).
///
/// Only counts imports whose bindings are actually referenced inside the function,
/// not all file-level imports. This prevents functions from being penalized for
/// imports they don't use.
pub fn compute_dc_from_events(
    events: &[QualitasEvent],
    imports: &[IrImportRecordType],
) -> DependencyCouplingResult {
    let api_calls = collect_api_calls(events);
    let referenced = collect_referenced_names(events);
    let used_imports = filter_used_imports(imports, &referenced);
    let (external_packages, internal_modules) = classify_used_imports(&used_imports);
    let import_count = used_imports.len() as u32;
    let distinct_api_calls = api_calls.len() as u32;
    build_dc_result(
        external_packages,
        internal_modules,
        import_count,
        distinct_api_calls,
    )
}

/// Classify used imports into external packages and internal modules.
fn classify_used_imports(imports: &[&IrImportRecordType]) -> (HashSet<String>, HashSet<String>) {
    let mut external_packages = HashSet::new();
    let mut internal_modules = HashSet::new();
    for import in imports {
        if import.is_external {
            external_packages.insert(root_package_name(&import.source));
        } else {
            internal_modules.insert(import.source.clone());
        }
    }
    (external_packages, internal_modules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::events::ApiCallEvent;
    use crate::ir::language::ImportRecord as IrImportRecordType;

    #[test]
    fn root_package_scoped() {
        assert_eq!(root_package_name("@scope/pkg/sub"), "@scope/pkg");
    }

    #[test]
    fn root_package_simple() {
        assert_eq!(root_package_name("react"), "react");
    }

    // ── Event-based tests ───────────────────────────────────────────────

    #[test]
    fn event_no_api_calls() {
        let events: Vec<QualitasEvent> = vec![];
        let imports: Vec<IrImportRecordType> = vec![];
        let r = compute_dc_from_events(&events, &imports);
        assert_eq!(r.distinct_api_calls, 0);
        assert_eq!(r.import_count, 0);
    }

    #[test]
    fn event_counts_api_calls() {
        let events = vec![
            QualitasEvent::ApiCall(ApiCallEvent {
                object: "fs".into(),
                method: "readFile".into(),
            }),
            QualitasEvent::ApiCall(ApiCallEvent {
                object: "fs".into(),
                method: "writeFile".into(),
            }),
            // Duplicate — should not increase distinct count
            QualitasEvent::ApiCall(ApiCallEvent {
                object: "fs".into(),
                method: "readFile".into(),
            }),
        ];
        let imports = vec![IrImportRecordType {
            source: "fs".into(),
            is_external: true,
            names: vec!["fs".into()],
        }];
        let r = compute_dc_from_events(&events, &imports);
        assert_eq!(r.distinct_api_calls, 2); // fs.readFile + fs.writeFile
        assert_eq!(r.import_count, 1);
        assert!(r.external_ratio > 0.0);
    }

    #[test]
    fn event_only_counts_used_imports() {
        use crate::ir::events::IdentEvent;

        // Function only references `fs`, not `path` or `util`
        let events = vec![
            QualitasEvent::IdentReference(IdentEvent {
                name: "fs".into(),
                byte_offset: 10,
            }),
            QualitasEvent::ApiCall(ApiCallEvent {
                object: "fs".into(),
                method: "readFile".into(),
            }),
        ];
        let imports = vec![
            IrImportRecordType {
                source: "fs".into(),
                is_external: true,
                names: vec!["fs".into()],
            },
            IrImportRecordType {
                source: "path".into(),
                is_external: true,
                names: vec!["join".into(), "resolve".into()],
            },
            IrImportRecordType {
                source: "./utils".into(),
                is_external: false,
                names: vec!["helper".into()],
            },
        ];
        let r = compute_dc_from_events(&events, &imports);
        // Only `fs` is used — path and ./utils are not referenced
        assert_eq!(r.import_count, 1);
        assert_eq!(r.distinct_api_calls, 1);
    }
}
