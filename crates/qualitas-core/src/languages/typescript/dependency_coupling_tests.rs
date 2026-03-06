use crate::analyzer::analyze_source_str;
use crate::types::AnalysisOptions;

fn default_options() -> AnalysisOptions {
    AnalysisOptions::default()
}

#[test]
fn ts_file_dependencies_count_all_imports() {
    let source = r"
import { readFile } from 'fs';
import { join } from 'path';
import { parse } from 'url';

function reader() {
    return readFile('test');
}
";
    let report = analyze_source_str(source, "imports.ts", &default_options()).unwrap();
    assert_eq!(
        report.file_dependencies.import_count, 3,
        "File-level should count all 3 imports, got {}",
        report.file_dependencies.import_count,
    );
}

#[test]
fn ts_function_only_counts_its_own_imports() {
    let source = r"
import { readFile } from 'fs';
import { join, resolve } from 'path';
import { parse } from 'url';

function reader() {
    return readFile('test');
}

function pathHelper() {
    return join(resolve('.'), 'out');
}
";
    let report = analyze_source_str(source, "split.ts", &default_options()).unwrap();

    assert_eq!(report.file_dependencies.import_count, 3);

    let reader = report
        .functions
        .iter()
        .find(|f| f.name == "reader")
        .unwrap();
    let path_helper = report
        .functions
        .iter()
        .find(|f| f.name == "pathHelper")
        .unwrap();

    assert_eq!(
        reader.metrics.dependency_coupling.import_count, 1,
        "reader() should count 1 import (fs), got {}",
        reader.metrics.dependency_coupling.import_count,
    );
    assert_eq!(
        path_helper.metrics.dependency_coupling.import_count, 1,
        "pathHelper() should count 1 import (path), got {}",
        path_helper.metrics.dependency_coupling.import_count,
    );
}

#[test]
fn ts_unused_imports_not_attributed_to_function() {
    let source = r"
import { readFile } from 'fs';
import { join } from 'path';
import { parse } from 'url';
import { hostname } from 'os';

function reader() {
    return readFile('test');
}
";
    let report = analyze_source_str(source, "unused.ts", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 4);

    let reader = &report.functions[0];
    assert_eq!(
        reader.metrics.dependency_coupling.import_count, 1,
        "reader() should only count 1 import, not all 4",
    );
}

#[test]
fn ts_file_scope_counts_its_own_imports() {
    let source = r"
import { readFile } from 'fs';
import { join } from 'path';
import { parse } from 'url';

if (typeof window === 'undefined') {
    readFile('config.json');
}

function pathHelper() {
    return join('.', 'out');
}
";
    let report = analyze_source_str(source, "scope.ts", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 3);

    let fs = report.file_scope.as_ref().expect("should have file-scope");
    assert_eq!(
        fs.metrics.dependency_coupling.import_count, 1,
        "file-scope should count 1 import (fs), got {}",
        fs.metrics.dependency_coupling.import_count,
    );

    let helper = &report.functions[0];
    assert_eq!(
        helper.metrics.dependency_coupling.import_count, 1,
        "pathHelper() should count 1 import (path), got {}",
        helper.metrics.dependency_coupling.import_count,
    );
}

#[test]
fn ts_import_attribution_sums_to_subset_of_file_imports() {
    let source = r"
import { readFile } from 'fs';
import { join, resolve } from 'path';
import { parse } from 'url';
import { hostname } from 'os';

if (typeof window === 'undefined') {
    readFile('config.json');
}

function pathHelper() {
    return join(resolve('.'), 'out');
}

function urlParser(input: string) {
    return parse(input);
}
";
    let report = analyze_source_str(source, "attribution.ts", &default_options()).unwrap();

    let file_imports = report.file_dependencies.import_count;
    assert_eq!(file_imports, 4);

    let fs_imports = report
        .file_scope
        .as_ref()
        .map_or(0, |fs| fs.metrics.dependency_coupling.import_count);
    let fn_imports: u32 = report
        .functions
        .iter()
        .map(|f| f.metrics.dependency_coupling.import_count)
        .sum();

    assert_eq!(fs_imports, 1, "file-scope uses fs");
    assert_eq!(fn_imports, 2, "functions use path + url");

    let attributed = fs_imports + fn_imports;
    assert!(
        attributed <= file_imports,
        "Attributed imports ({attributed}) should be <= file imports ({file_imports})",
    );
    assert_eq!(
        attributed,
        file_imports - 1,
        "os import is unused, so attributed should be file_imports - 1",
    );
}
