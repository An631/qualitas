/// Dependency Coupling (DC) metric
///
/// Measures how many external dependencies and distinct APIs a file/function touches.
#[cfg(test)]
use oxc_ast::ast::*;
#[cfg(test)]
use oxc_ast::visit::walk;
#[cfg(test)]
use oxc_ast::Visit;
use std::collections::HashSet;

use crate::ir::language::ImportRecord as IrImportRecordType;
#[cfg(test)]
use crate::parser::ast::ImportRecord;
use crate::types::DependencyCouplingResult;

/// Analyze file-level import dependencies (from IR import records).
pub fn analyze_file_dependencies_ir(imports: &[IrImportRecordType]) -> DependencyCouplingResult {
    let mut external_packages: HashSet<String> = HashSet::new();
    let mut internal_modules: HashSet<String> = HashSet::new();

    for import in imports {
        if import.is_external {
            external_packages.insert(root_package_name(&import.source));
        } else {
            internal_modules.insert(import.source.clone());
        }
    }

    let import_count = imports.len() as u32;
    let distinct_sources = (external_packages.len() + internal_modules.len()) as u32;
    let external_ratio = if import_count > 0 {
        external_packages.len() as f64 / import_count as f64
    } else {
        0.0
    };

    let raw_score = compute_dc_raw(import_count, external_ratio, 0);

    DependencyCouplingResult {
        import_count,
        distinct_sources,
        external_ratio,
        external_packages: external_packages.into_iter().collect(),
        internal_modules: internal_modules.into_iter().collect(),
        distinct_api_calls: 0,
        closure_captures: 0,
        raw_score,
    }
}

/// Analyze file-level import dependencies (from parser import records).
#[cfg(test)]
pub fn analyze_file_dependencies(imports: &[ImportRecord]) -> DependencyCouplingResult {
    let mut external_packages: HashSet<String> = HashSet::new();
    let mut internal_modules: HashSet<String> = HashSet::new();

    for import in imports {
        if import.is_external {
            external_packages.insert(root_package_name(&import.source));
        } else {
            internal_modules.insert(import.source.clone());
        }
    }

    let import_count = imports.len() as u32;
    let distinct_sources = (external_packages.len() + internal_modules.len()) as u32;
    let external_ratio = if import_count > 0 {
        external_packages.len() as f64 / import_count as f64
    } else {
        0.0
    };

    let raw_score = compute_dc_raw(import_count, external_ratio, 0);

    DependencyCouplingResult {
        import_count,
        distinct_sources,
        external_ratio,
        external_packages: external_packages.into_iter().collect(),
        internal_modules: internal_modules.into_iter().collect(),
        distinct_api_calls: 0,
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

/// Collect all imported binding names.
#[cfg(test)]
pub fn collect_imported_names(imports: &[ImportRecord]) -> HashSet<String> {
    imports.iter().flat_map(|r| r.names.iter().cloned()).collect()
}

/// Analyze function-level API call patterns.
#[cfg(test)]
pub fn analyze_function_dependencies(
    body: &FunctionBody<'_>,
    imported_names: &HashSet<String>,
) -> DependencyCouplingResult {
    let mut visitor = DcFunctionVisitor {
        imported_names,
        api_calls: HashSet::new(),
    };
    visitor.visit_function_body(body);

    let distinct_api_calls = visitor.api_calls.len() as u32;
    let raw_score = compute_dc_raw(0, 0.0, distinct_api_calls);

    DependencyCouplingResult {
        import_count: 0,
        distinct_sources: 0,
        external_ratio: 0.0,
        external_packages: Vec::new(),
        internal_modules: Vec::new(),
        distinct_api_calls,
        closure_captures: 0,
        raw_score,
    }
}

pub fn compute_dc_raw(import_count: u32, external_ratio: f64, distinct_api_calls: u32) -> f64 {
    use crate::constants::*;
    DC_IMPORT_WEIGHT * (import_count as f64 / NORM_DC_IMPORTS)
        + DC_EXTERNAL_RATIO_WEIGHT * external_ratio
        + DC_API_CALLS_WEIGHT * (distinct_api_calls as f64 / NORM_DC_API_CALLS)
}

// ─── Event-based DC computation ─────────────────────────────────────────────

use crate::ir::events::*;
use crate::ir::language::ImportRecord as IrImportRecord;

/// Compute function-level DC from a stream of IR events (language-agnostic).
///
/// `imports` provides file-level import context for computing the full DC score.
pub fn compute_dc_from_events(
    events: &[QualitasEvent],
    imports: &[IrImportRecord],
) -> DependencyCouplingResult {
    let mut api_calls: HashSet<String> = HashSet::new();

    for event in events {
        if let QualitasEvent::ApiCall(call) = event {
            let key = format!("{}.{}", call.object, call.method);
            api_calls.insert(key);
        }
    }

    // File-level stats from imports
    let mut external_packages: HashSet<String> = HashSet::new();
    let mut internal_modules: HashSet<String> = HashSet::new();

    for import in imports {
        if import.is_external {
            external_packages.insert(root_package_name(&import.source));
        } else {
            internal_modules.insert(import.source.clone());
        }
    }

    let import_count = imports.len() as u32;
    let distinct_sources = (external_packages.len() + internal_modules.len()) as u32;
    let external_ratio = if import_count > 0 {
        external_packages.len() as f64 / import_count as f64
    } else {
        0.0
    };

    let distinct_api_calls = api_calls.len() as u32;
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

#[cfg(test)]
struct DcFunctionVisitor<'a> {
    imported_names: &'a HashSet<String>,
    api_calls: HashSet<String>,
}

#[cfg(test)]
impl<'a, 'b> Visit<'b> for DcFunctionVisitor<'a> {
    fn visit_call_expression(&mut self, it: &CallExpression<'b>) {
        if let Expression::StaticMemberExpression(member) = &it.callee {
            if let Expression::Identifier(obj) = &member.object {
                let obj_name = obj.name.as_str();
                if self.imported_names.contains(obj_name) {
                    let key = format!("{}.{}", obj_name, member.property.name.as_str());
                    self.api_calls.insert(key);
                }
            }
        }
        walk::walk_call_expression(self, it);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let imports: Vec<IrImportRecord> = vec![];
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
        let imports = vec![IrImportRecord {
            source: "fs".into(),
            is_external: true,
            names: vec!["fs".into()],
        }];
        let r = compute_dc_from_events(&events, &imports);
        assert_eq!(r.distinct_api_calls, 2); // fs.readFile + fs.writeFile
        assert_eq!(r.import_count, 1);
        assert!(r.external_ratio > 0.0);
    }
}
