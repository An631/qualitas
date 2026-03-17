use crate::analyzer::analyze_source_str;
use crate::types::AnalysisOptions;

fn default_options() -> AnalysisOptions {
    AnalysisOptions::default()
}

#[test]
fn go_file_dependencies_count_all_imports() {
    let source = r#"
package main

import (
    "fmt"
    "os"
    "strings"
)

func greet() {
    fmt.Println("hello")
}
"#;
    let report = analyze_source_str(source, "imports.go", &default_options()).unwrap();
    assert_eq!(
        report.file_dependencies.import_count, 3,
        "File-level should count all 3 imports, got {}",
        report.file_dependencies.import_count,
    );
}

#[test]
fn go_function_only_counts_its_own_imports() {
    let source = r#"
package main

import (
    "fmt"
    "os"
    "strings"
)

func useFmt() {
    fmt.Println("hello")
}

func useOs() string {
    dir, _ := os.Getwd()
    return dir
}
"#;
    let report = analyze_source_str(source, "split.go", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 3);

    let use_fmt = report
        .functions
        .iter()
        .find(|f| f.name == "useFmt")
        .unwrap();
    let use_os = report.functions.iter().find(|f| f.name == "useOs").unwrap();

    assert_eq!(
        use_fmt.metrics.dependency_coupling.import_count, 1,
        "useFmt() should count 1 import (fmt), got {}",
        use_fmt.metrics.dependency_coupling.import_count,
    );
    assert_eq!(
        use_os.metrics.dependency_coupling.import_count, 1,
        "useOs() should count 1 import (os), got {}",
        use_os.metrics.dependency_coupling.import_count,
    );
}

#[test]
fn go_unused_imports_not_attributed_to_function() {
    let source = r#"
package main

import (
    "fmt"
    "os"
    "strings"
)

func useFmt() {
    fmt.Println("hello")
}
"#;
    let report = analyze_source_str(source, "unused.go", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 3);

    let use_fmt = &report.functions[0];
    assert_eq!(
        use_fmt.metrics.dependency_coupling.import_count, 1,
        "useFmt() should only count 1 import, not all 3",
    );
}

#[test]
fn go_import_with_alias() {
    let source = r#"
package main

import (
    f "fmt"
)

func greet() {
    f.Println("hello")
}
"#;
    let report = analyze_source_str(source, "alias.go", &default_options()).unwrap();
    let greet = &report.functions[0];
    assert_eq!(
        greet.metrics.dependency_coupling.import_count, 1,
        "Aliased import should be tracked by alias name",
    );
}

#[test]
fn go_no_file_scope() {
    let source = r#"
package main

import "fmt"

func hello() {
    fmt.Println("hello")
}
"#;
    let report = analyze_source_str(source, "noscope.go", &default_options()).unwrap();
    assert!(
        report.file_scope.is_none(),
        "Go should not produce file-scope analysis",
    );
}
