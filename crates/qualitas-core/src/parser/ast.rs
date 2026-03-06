#[cfg(test)]
use oxc_allocator::Allocator;
#[cfg(test)]
use oxc_ast::ast::*;
#[cfg(test)]
use oxc_ast::visit::walk;
#[cfg(test)]
use oxc_ast::Visit;
#[cfg(test)]
use oxc_parser::Parser;
#[cfg(test)]
use oxc_span::SourceType;
#[cfg(test)]
use oxc_syntax::scope::ScopeFlags;

/// Info about a parsed function boundary (legacy — used by tests only).
#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct FunctionInfo {
    pub name: String,
    pub inferred_name: Option<String>,
    /// Byte offset of function start in source
    pub start: u32,
    /// Byte offset of function end in source
    pub end: u32,
    pub is_async: bool,
    pub is_generator: bool,
    pub depth: u32,
}

#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug)]
pub struct ClassInfo {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub methods: Vec<FunctionInfo>,
}

/// Parsed representation of a source file (legacy — used by tests only).
#[cfg(test)]
#[allow(dead_code)]
pub struct ParsedFile {
    pub source: String,
    pub top_level_functions: Vec<FunctionInfo>,
    pub classes: Vec<ClassInfo>,
    pub import_records: Vec<ImportRecord>,
}

#[cfg(test)]
#[derive(Debug)]
pub struct ImportRecord {
    pub source: String,
    pub is_external: bool,
    /// Imported binding names
    pub names: Vec<String>,
}

/// Parse source text and extract function/class boundaries + import records.
#[cfg(test)]
pub fn parse_source(source: &str, file_name: &str) -> Result<ParsedFile, String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(file_name)
        .unwrap_or_else(|_| SourceType::default().with_typescript(true));

    let result = Parser::new(&allocator, source, source_type).parse();

    if !result.errors.is_empty() {
        let msg = result
            .errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        eprintln!("qualitas parse warning for {file_name}: {msg}");
    }

    let mut collector = BoundaryCollector {
        top_level_functions: Vec::new(),
        classes: Vec::new(),
        imports: Vec::new(),
        depth: 0,
        class_stack: Vec::new(),
    };
    collector.visit_program(&result.program);

    Ok(ParsedFile {
        source: source.to_string(),
        top_level_functions: collector.top_level_functions,
        classes: collector.classes,
        import_records: collector.imports,
    })
}

// ─── Boundary collector ───────────────────────────────────────────────────────

#[cfg(test)]
struct BoundaryCollector {
    top_level_functions: Vec<FunctionInfo>,
    classes: Vec<ClassInfo>,
    imports: Vec<ImportRecord>,
    depth: u32,
    class_stack: Vec<usize>,
}

#[cfg(test)]
impl BoundaryCollector {
    fn push_function(&mut self, info: FunctionInfo) {
        if let Some(&class_idx) = self.class_stack.last() {
            self.classes[class_idx].methods.push(info);
        } else {
            self.top_level_functions.push(info);
        }
    }
}

/// Extract the local binding name from an import specifier.
#[cfg(test)]
fn specifier_local_name(spec: &ImportDeclarationSpecifier<'_>) -> String {
    match spec {
        ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => s.local.name.to_string(),
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => s.local.name.to_string(),
        ImportDeclarationSpecifier::ImportSpecifier(s) => s.local.name.to_string(),
    }
}

/// Try to extract function-like metadata from an expression (arrow or function expression).
/// Returns `(start, end, is_async, is_generator)` or `None` if not function-like.
#[cfg(test)]
fn extract_function_expr_info(expr: &Expression<'_>) -> Option<(u32, u32, bool, bool)> {
    match expr {
        Expression::ArrowFunctionExpression(a) => {
            Some((a.span.start, a.span.end, a.r#async, false))
        }
        Expression::FunctionExpression(f) => {
            Some((f.span.start, f.span.end, f.r#async, f.generator))
        }
        _ => None,
    }
}

#[cfg(test)]
impl<'a> Visit<'a> for BoundaryCollector {
    fn visit_import_declaration(&mut self, decl: &ImportDeclaration<'a>) {
        let source_str = decl.source.value.as_str();
        let is_ext = !source_str.starts_with('.') && !source_str.starts_with('/');

        let names: Vec<String> = decl
            .specifiers
            .as_ref()
            .map(|specs| specs.iter().map(specifier_local_name).collect())
            .unwrap_or_default();

        self.imports.push(ImportRecord {
            source: source_str.to_string(),
            is_external: is_ext,
            names,
        });
    }

    fn visit_function(&mut self, func: &Function<'a>, flags: ScopeFlags) {
        let name = func
            .id
            .as_ref()
            .map(|id| id.name.to_string())
            .unwrap_or_else(|| "(anonymous)".to_string());

        let info = FunctionInfo {
            name,
            inferred_name: None,
            start: func.span.start,
            end: func.span.end,
            is_async: func.r#async,
            is_generator: func.generator,
            depth: self.depth,
        };
        self.push_function(info);

        self.depth += 1;
        walk::walk_function(self, func, flags);
        self.depth -= 1;
    }

    fn visit_variable_declarator(&mut self, decl: &VariableDeclarator<'a>) {
        let name = match &decl.id.kind {
            BindingPatternKind::BindingIdentifier(id) => id.name.to_string(),
            _ => {
                walk::walk_variable_declarator(self, decl);
                return;
            }
        };

        if let Some(init) = &decl.init {
            if let Some((start, end, is_async, is_gen)) = extract_function_expr_info(init) {
                let info = FunctionInfo {
                    name: name.clone(),
                    inferred_name: Some(format!("const {name} = ")),
                    start,
                    end,
                    is_async,
                    is_generator: is_gen,
                    depth: self.depth,
                };
                self.push_function(info);
            }
        }

        walk::walk_variable_declarator(self, decl);
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        let name = class
            .id
            .as_ref()
            .map(|id| id.name.to_string())
            .unwrap_or_else(|| "(anonymous class)".to_string());

        let class_info = ClassInfo {
            name,
            start: class.span.start,
            end: class.span.end,
            methods: Vec::new(),
        };
        let idx = self.classes.len();
        self.classes.push(class_info);
        self.class_stack.push(idx);

        walk::walk_class(self, class);

        self.class_stack.pop();
    }
}

/// Convert byte offset to 1-based line number.
pub fn byte_to_line(source: &str, offset: u32) -> u32 {
    let offset = (offset as usize).min(source.len());
    source[..offset].chars().filter(|&c| c == '\n').count() as u32 + 1
}

/// Count non-blank, non-comment source lines in a byte range.
pub fn count_loc(source: &str, start: u32, end: u32) -> u32 {
    let start = start as usize;
    let end = (end as usize).min(source.len());
    source[start..end]
        .lines()
        .filter(|line| {
            let t = line.trim();
            !t.is_empty() && !t.starts_with("//") && !t.starts_with('*') && !t.starts_with("/*")
        })
        .count() as u32
}
