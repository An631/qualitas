use crate::analyzer::analyze_source_str;
use crate::types::AnalysisOptions;

fn default_options() -> AnalysisOptions {
    AnalysisOptions::default()
}

/// Helper: find a method by name across all classes in the report.
fn find_method<'a>(
    report: &'a crate::types::FileQualityReport,
    name: &str,
) -> &'a crate::types::FunctionQualityReport {
    // Check top-level functions first
    if let Some(f) = report.functions.iter().find(|f| f.name == name) {
        return f;
    }
    // Check class methods
    for class in &report.classes {
        if let Some(m) = class.methods.iter().find(|m| m.name == name) {
            return m;
        }
    }
    panic!("Method '{name}' not found in report")
}

#[test]
fn java_file_dependencies_count_all_imports() {
    let source = r#"
import java.util.List;
import java.util.Map;
import java.io.File;

public class Test {
    public void use_list() {
        List.of("a");
    }
}
"#;
    let report = analyze_source_str(source, "Imports.java", &default_options()).unwrap();
    assert_eq!(
        report.file_dependencies.import_count, 3,
        "File-level should count all 3 imports, got {}",
        report.file_dependencies.import_count,
    );
}

#[test]
fn java_method_only_counts_its_own_imports() {
    let source = r#"
import java.util.List;
import java.io.File;
import java.util.Map;

public class Test {
    public void useList() {
        List.of("a");
    }

    public void useFile() {
        File.createTempFile("a", "b");
    }
}
"#;
    let report = analyze_source_str(source, "Split.java", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 3);

    let use_list = find_method(&report, "useList");
    let use_file = find_method(&report, "useFile");

    assert_eq!(
        use_list.metrics.dependency_coupling.import_count, 1,
        "useList() should count 1 import (List), got {}",
        use_list.metrics.dependency_coupling.import_count,
    );
    assert_eq!(
        use_file.metrics.dependency_coupling.import_count, 1,
        "useFile() should count 1 import (File), got {}",
        use_file.metrics.dependency_coupling.import_count,
    );
}

#[test]
fn java_unused_imports_not_attributed() {
    let source = r#"
import java.util.List;
import java.io.File;
import java.util.Map;

public class Test {
    public void useList() {
        List.of("a");
    }
}
"#;
    let report = analyze_source_str(source, "Unused.java", &default_options()).unwrap();
    assert_eq!(report.file_dependencies.import_count, 3);

    let use_list = find_method(&report, "useList");
    assert_eq!(
        use_list.metrics.dependency_coupling.import_count, 1,
        "useList() should only count 1 import, not all 3",
    );
}

#[test]
fn java_static_import_tracked() {
    let source = r"
import static java.lang.Math.abs;

public class Test {
    public int positive(int x) {
        return abs(x);
    }
}
";
    let report = analyze_source_str(source, "Static.java", &default_options()).unwrap();
    assert_eq!(
        report.file_dependencies.import_count, 1,
        "Should have 1 static import",
    );
}

#[test]
fn java_wildcard_import_constructor_tracked() {
    let source = r"
import java.net.*;

public class Test {
    public void fetch(String urlStr) throws Exception {
        URL url = new URL(urlStr);
    }
}
";
    let report = analyze_source_str(source, "Wildcard.java", &default_options()).unwrap();
    let method = find_method(&report, "fetch");
    assert!(
        method.metrics.dependency_coupling.import_count >= 1,
        "new URL() should attribute the java.net.* import, got import_count={}",
        method.metrics.dependency_coupling.import_count,
    );
    assert!(
        method.metrics.dependency_coupling.distinct_api_calls >= 1,
        "new URL() should count as an API call, got distinct_api_calls={}",
        method.metrics.dependency_coupling.distinct_api_calls,
    );
}

#[test]
fn java_wildcard_import_static_call_tracked() {
    let source = r"
import java.time.*;

public class Test {
    public void logTime() {
        LocalDateTime now = LocalDateTime.now();
    }
}
";
    let report = analyze_source_str(source, "Static.java", &default_options()).unwrap();
    let method = find_method(&report, "logTime");
    assert!(
        method.metrics.dependency_coupling.import_count >= 1,
        "LocalDateTime.now() should attribute the java.time.* import, got import_count={}",
        method.metrics.dependency_coupling.import_count,
    );
    assert!(
        method.metrics.dependency_coupling.distinct_api_calls >= 1,
        "LocalDateTime.now() should count as an API call, got distinct_api_calls={}",
        method.metrics.dependency_coupling.distinct_api_calls,
    );
}

#[test]
fn java_wildcard_import_multiple_packages_tracked() {
    let source = r#"
import java.net.*;
import java.io.*;
import java.time.*;
import java.util.logging.*;

public class Test {
    public void fetchAndStore(String urlStr) throws Exception {
        URL url = new URL(urlStr);
        HttpURLConnection conn = (HttpURLConnection) url.openConnection();
        conn.setRequestMethod("GET");
        InputStream stream = conn.getInputStream();
        BufferedReader reader = new BufferedReader(new InputStreamReader(stream));
        LocalDateTime now = LocalDateTime.now();
        Logger.getLogger("test").info("done");
    }
}
"#;
    let report = analyze_source_str(source, "Multi.java", &default_options()).unwrap();
    let method = find_method(&report, "fetchAndStore");

    // Should have multiple imports attributed (java.net, java.io, java.time, java.util.logging)
    assert!(
        method.metrics.dependency_coupling.import_count >= 3,
        "Should attribute at least 3 wildcard imports, got import_count={}",
        method.metrics.dependency_coupling.import_count,
    );

    // Should have multiple distinct API calls (new URL, new BufferedReader, new InputStreamReader,
    // LocalDateTime.now, Logger.getLogger, etc.)
    assert!(
        method.metrics.dependency_coupling.distinct_api_calls >= 3,
        "Should have at least 3 distinct API calls, got distinct_api_calls={}",
        method.metrics.dependency_coupling.distinct_api_calls,
    );
}

#[test]
fn java_specific_import_still_works_with_wildcards() {
    // Mix of specific and wildcard imports
    let source = r#"
import java.util.List;
import java.net.*;

public class Test {
    public void fetch(String urlStr) throws Exception {
        List.of("a");
        URL url = new URL(urlStr);
    }
}
"#;
    let report = analyze_source_str(source, "Mixed.java", &default_options()).unwrap();
    let method = find_method(&report, "fetch");
    assert_eq!(
        method.metrics.dependency_coupling.import_count, 2,
        "Should attribute both specific and wildcard imports, got {}",
        method.metrics.dependency_coupling.import_count,
    );
}
