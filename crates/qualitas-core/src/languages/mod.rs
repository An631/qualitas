/// Language adapter registry.
///
/// Each supported language registers its adapter here.
/// Language detection is done by file extension.
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::ir::language::LanguageAdapter;

// Register language adapters here:
pub mod rust;
pub mod typescript;
// pub mod python;

/// All registered language adapters.
fn all_adapters() -> Vec<Box<dyn LanguageAdapter>> {
    vec![
        Box::new(typescript::TypeScriptAdapter),
        Box::new(rust::RustAdapter),
        // Box::new(python::PythonAdapter),
    ]
}

type AdapterRegistry = (Vec<Box<dyn LanguageAdapter>>, HashMap<String, usize>);

/// Extension-to-adapter index, lazily initialized.
static REGISTRY: OnceLock<AdapterRegistry> = OnceLock::new();

fn registry() -> &'static AdapterRegistry {
    REGISTRY.get_or_init(|| {
        let adapters = all_adapters();
        let mut ext_map = HashMap::new();
        for (idx, adapter) in adapters.iter().enumerate() {
            for ext in adapter.extensions() {
                ext_map.insert(ext.to_string(), idx);
            }
        }
        (adapters, ext_map)
    })
}

/// Look up the language adapter for a given file path by extension.
///
/// Returns `Err` if no adapter is registered for the file's extension.
pub fn adapter_for_file(file_path: &str) -> Result<&'static dyn LanguageAdapter, String> {
    let (adapters, ext_map) = registry();

    let ext = file_path
        .rsplit('.')
        .next()
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    ext_map
        .get(&ext)
        .map(|&idx| adapters[idx].as_ref())
        .ok_or_else(|| format!("Unsupported file type: {ext}"))
}

/// List all registered language adapters.
#[allow(dead_code)]
pub fn list_adapters() -> &'static [Box<dyn LanguageAdapter>] {
    let (adapters, _) = registry();
    adapters
}
