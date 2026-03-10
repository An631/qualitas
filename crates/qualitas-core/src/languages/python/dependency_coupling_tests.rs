use crate::analyzer::analyze_source_str;
use crate::types::AnalysisOptions;

fn default_options() -> AnalysisOptions {
    AnalysisOptions::default()
}

#[test]
fn py_file_dependencies_count_all_imports() {
    let source = r#"
import os
import sys
from pathlib import Path

def get_path():
    return Path(".")
"#;
    let report = analyze_source_str(source, "imports.py", &default_options()).unwrap();
    assert_eq!(
        report.file_dependencies.import_count, 3,
        "File-level should count all 3 import statements, got {}",
        report.file_dependencies.import_count,
    );
}

#[test]
fn py_function_only_counts_its_own_imports() {
    let source = r#"
import os
from pathlib import Path
from sys import argv

def use_path():
    return Path(".")

def use_os():
    return os.getcwd()
"#;
    let report = analyze_source_str(source, "split.py", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 3);

    let use_path = report
        .functions
        .iter()
        .find(|f| f.name == "use_path")
        .unwrap();
    let use_os = report
        .functions
        .iter()
        .find(|f| f.name == "use_os")
        .unwrap();

    assert_eq!(
        use_path.metrics.dependency_coupling.import_count, 1,
        "use_path() should count 1 import (Path), got {}",
        use_path.metrics.dependency_coupling.import_count,
    );
    assert_eq!(
        use_os.metrics.dependency_coupling.import_count, 1,
        "use_os() should count 1 import (os), got {}",
        use_os.metrics.dependency_coupling.import_count,
    );
}

#[test]
fn py_unused_imports_not_attributed_to_function() {
    let source = r"
import os
import sys
from pathlib import Path
from collections import OrderedDict

def use_os():
    return os.getcwd()
";
    let report = analyze_source_str(source, "unused.py", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 4);

    let use_os = &report.functions[0];
    assert_eq!(
        use_os.metrics.dependency_coupling.import_count, 1,
        "use_os() should only count 1 import, not all 4",
    );
}

#[test]
fn py_from_import_with_alias() {
    let source = r#"
from pathlib import Path as P

def make_path():
    return P(".")
"#;
    let report = analyze_source_str(source, "alias.py", &default_options()).unwrap();
    let make_path = &report.functions[0];
    assert_eq!(
        make_path.metrics.dependency_coupling.import_count, 1,
        "Aliased import should be tracked by alias name",
    );
}

#[test]
fn py_no_file_scope_for_python() {
    let source = r"
import os

def hello():
    return os.getcwd()
";
    let report = analyze_source_str(source, "noscope.py", &default_options()).unwrap();
    assert!(
        report.file_scope.is_none(),
        "Python should not produce file-scope analysis",
    );
}
