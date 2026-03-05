use crate::analyzer::analyze_source_str;
use crate::types::AnalysisOptions;

fn default_options() -> AnalysisOptions {
    AnalysisOptions::default()
}

#[test]
fn rs_file_dependencies_count_all_imports() {
    let source = r"
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

fn make_map() -> HashMap<String, String> {
    HashMap::new()
}
";
    let report = analyze_source_str(source, "imports.rs", &default_options()).unwrap();
    assert_eq!(
        report.file_dependencies.import_count, 3,
        "File-level should count all 3 use statements, got {}",
        report.file_dependencies.import_count,
    );
}

#[test]
fn rs_function_only_counts_its_own_imports() {
    let source = r"
use std::collections::HashMap;
use std::path::PathBuf;
use std::io::Read;

fn make_map() -> HashMap<String, String> {
    HashMap::new()
}

fn make_path() -> PathBuf {
    PathBuf::new()
}
";
    let report = analyze_source_str(source, "split.rs", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 3);

    let make_map = report
        .functions
        .iter()
        .find(|f| f.name == "make_map")
        .unwrap();
    let make_path = report
        .functions
        .iter()
        .find(|f| f.name == "make_path")
        .unwrap();

    assert_eq!(
        make_map.metrics.dependency_coupling.import_count, 1,
        "make_map() should count 1 import (HashMap), got {}",
        make_map.metrics.dependency_coupling.import_count,
    );
    assert_eq!(
        make_path.metrics.dependency_coupling.import_count, 1,
        "make_path() should count 1 import (PathBuf), got {}",
        make_path.metrics.dependency_coupling.import_count,
    );
}

#[test]
fn rs_unused_imports_not_attributed_to_function() {
    let source = r"
use std::collections::HashMap;
use std::path::PathBuf;
use std::io::Read;
use std::net::TcpStream;

fn make_map() -> HashMap<String, String> {
    HashMap::new()
}
";
    let report = analyze_source_str(source, "unused.rs", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 4);

    let make_map = &report.functions[0];
    assert_eq!(
        make_map.metrics.dependency_coupling.import_count, 1,
        "make_map() should only count 1 import, not all 4",
    );
}

#[test]
fn rs_no_file_scope_for_rust() {
    let source = r"
use std::collections::HashMap;

fn make_map() -> HashMap<String, String> {
    HashMap::new()
}
";
    let report = analyze_source_str(source, "noscope.rs", &default_options()).unwrap();
    assert!(
        report.file_scope.is_none(),
        "Rust should not produce file-scope analysis",
    );
}
