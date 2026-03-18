/// Go language adapter.
///
/// Uses `tree-sitter-go` for AST analysis and emits `QualitasEvent`s
/// for the language-agnostic metric collectors.
use std::collections::{HashMap, HashSet};

use tree_sitter::{Node, Parser};

use crate::ir::events::{
    ApiCallEvent, AsyncEvent, ControlFlowEvent, ControlFlowKind, IdentEvent, LogicOpEvent,
    OperandEvent, OperatorEvent, QualitasEvent,
};
use crate::ir::language::{
    ClassExtraction, FileExtraction, FunctionExtraction, ImportRecord, LanguageAdapter,
    ThresholdOverrides,
};

pub struct GoAdapter;

impl LanguageAdapter for GoAdapter {
    fn name(&self) -> &'static str {
        "Go"
    }

    fn extensions(&self) -> &[&str] {
        &[".go"]
    }

    fn test_patterns(&self) -> &[&str] {
        &["_test.go", "tests/", "tests\\"]
    }

    fn threshold_overrides(&self) -> Option<ThresholdOverrides> {
        Some(ThresholdOverrides {
            norm_cfc: Some(30.0),
            cfc_warning: Some(18),
            cfc_error: Some(25),
            ..Default::default()
        })
    }

    fn extract(&self, source: &str, file_path: &str) -> Result<FileExtraction, String> {
        let mut parser = Parser::new();
        let language = tree_sitter_go::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| format!("qualitas: failed to set Go language: {e}"))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| format!("qualitas parse error for {file_path}: failed to parse"))?;

        let root = tree.root_node();
        if root.has_error() {
            return Err(format!(
                "qualitas parse error for {file_path}: syntax error"
            ));
        }

        let mut extractor = GoExtractor::new(source);
        extractor.extract_top_level(&root);

        let classes = extractor.finalize_classes();
        Ok(FileExtraction {
            functions: extractor.functions,
            classes,
            imports: extractor.imports,
            file_scope: None,
        })
    }
}

// ─── Helper: get node text ───────────────────────────────────────────────────

fn node_text<'a>(node: &Node, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn node_text_owned(node: &Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

// ─── Top-level extractor ────────────────────────────────────────────────────

struct GoExtractor<'src> {
    source: &'src str,
    functions: Vec<FunctionExtraction>,
    /// Methods grouped by receiver type name.
    method_groups: HashMap<String, Vec<FunctionExtraction>>,
    /// Byte ranges for receiver type structs (for ClassExtraction span).
    struct_spans: HashMap<String, (u32, u32, u32, u32)>,
    imports: Vec<ImportRecord>,
    imported_names: HashSet<String>,
}

struct FunctionExtractionBuildOptions {
    name: String,
    param_count: u32,
    events: Vec<QualitasEvent>,
}

impl<'src> GoExtractor<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            functions: Vec::new(),
            method_groups: HashMap::new(),
            struct_spans: HashMap::new(),
            imports: Vec::new(),
            imported_names: HashSet::new(),
        }
    }

    fn extract_top_level(&mut self, root: &Node) {
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "function_declaration" => {
                    if let Some(fe) = self.extract_function(&child) {
                        self.functions.push(fe);
                    }
                }
                "method_declaration" => {
                    self.extract_method(&child);
                }
                "import_declaration" => self.extract_imports(&child),
                "type_declaration" => self.extract_type_decl(&child),
                _ => {}
            }
        }
    }

    fn extract_function(&self, node: &Node) -> Option<FunctionExtraction> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text_owned(&name_node, self.source);
        let params_node = node.child_by_field_name("parameters");
        let param_count = params_node.map_or(0, |p| count_params(&p, self.source));
        let body_node = node.child_by_field_name("body")?;

        let mut emitter = GoBodyEventEmitter::new(self.source, &name, &self.imported_names);
        emitter.visit_block(&body_node);

        Some(Self::build_function_extraction(
            node,
            &body_node,
            FunctionExtractionBuildOptions {
                name,
                param_count,
                events: emitter.events,
            },
        ))
    }

    fn extract_method(&mut self, node: &Node) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let name = node_text_owned(&name_node, self.source);
        let params_node = node.child_by_field_name("parameters");
        let param_count = params_node.map_or(0, |p| count_params(&p, self.source));
        let Some(body_node) = node.child_by_field_name("body") else {
            return;
        };

        let mut emitter = GoBodyEventEmitter::new(self.source, &name, &self.imported_names);
        emitter.visit_block(&body_node);

        let fe = Self::build_function_extraction(
            node,
            &body_node,
            FunctionExtractionBuildOptions {
                name,
                param_count,
                events: emitter.events,
            },
        );

        self.store_method_extraction(node, fe);
    }

    fn build_function_extraction(
        node: &Node,
        body_node: &Node,
        options: FunctionExtractionBuildOptions,
    ) -> FunctionExtraction {
        let FunctionExtractionBuildOptions {
            name,
            param_count,
            events,
        } = options;

        FunctionExtraction {
            name,
            inferred_name: None,
            byte_start: node.start_byte() as u32,
            byte_end: node.end_byte() as u32,
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            param_count,
            is_async: false,
            is_generator: false,
            events,
            loc_override: None,
            statement_count: Some(count_statements(body_node)),
        }
    }

    fn store_method_extraction(&mut self, node: &Node, method: FunctionExtraction) {
        if let Some(recv_type) = self.get_receiver_type(node) {
            self.method_groups
                .entry(recv_type)
                .or_default()
                .push(method);
            return;
        }
        self.functions.push(method);
    }

    fn get_receiver_type(&self, node: &Node) -> Option<String> {
        let receiver = node.child_by_field_name("receiver")?;
        // receiver is a parameter_list containing one parameter_declaration
        let mut cursor = receiver.walk();
        for child in receiver.named_children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                // The type could be `*MyStruct` or `MyStruct`
                if let Some(type_node) = child.child_by_field_name("type") {
                    return Some(extract_type_name(&type_node, self.source));
                }
            }
        }
        None
    }

    fn extract_imports(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.extract_import_child(&child);
        }
    }

    fn extract_import_child(&mut self, node: &Node) {
        match node.kind() {
            "import_spec" => self.extract_import_spec(node),
            "import_spec_list" => self.extract_import_spec_list(node),
            _ => {}
        }
    }

    fn extract_import_spec_list(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for spec in node.named_children(&mut cursor) {
            if spec.kind() != "import_spec" {
                continue;
            }
            self.extract_import_spec(&spec);
        }
    }

    fn extract_import_spec(&mut self, node: &Node) {
        let path = self.import_path(node);
        let Some(binding) = self.resolve_import_binding(node, &path) else {
            return;
        };
        self.record_import(path, binding);
    }

    fn import_path(&self, node: &Node) -> String {
        let Some(path_node) = node.child_by_field_name("path") else {
            return String::new();
        };
        let text = node_text(&path_node, self.source);
        text.trim_matches('"').to_string()
    }

    fn resolve_import_binding(&self, node: &Node, path: &str) -> Option<String> {
        let Some(alias_node) = node.child_by_field_name("name") else {
            return Some(Self::default_import_binding(path));
        };
        self.resolve_alias_binding(&alias_node, path)
    }

    fn resolve_alias_binding(&self, alias_node: &Node, path: &str) -> Option<String> {
        let alias_text = node_text(alias_node, self.source);
        if alias_text == "_" {
            return None;
        }
        if alias_text == "." {
            return Some(Self::default_import_binding(path));
        }
        Some(alias_text.to_string())
    }

    fn default_import_binding(path: &str) -> String {
        path.rsplit('/').next().unwrap_or(path).to_string()
    }

    fn record_import(&mut self, path: String, binding: String) {
        self.imported_names.insert(binding.clone());
        self.imports.push(ImportRecord {
            source: path,
            is_external: true,
            names: vec![binding],
        });
    }

    fn extract_type_decl(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "type_spec" {
                self.extract_type_spec(&child);
            }
        }
    }

    fn extract_type_spec(&mut self, node: &Node) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Some(type_node) = node.child_by_field_name("type") else {
            return;
        };

        if type_node.kind() == "struct_type" {
            let name = node_text_owned(&name_node, self.source);
            self.struct_spans.insert(
                name,
                (
                    node.start_byte() as u32,
                    node.end_byte() as u32,
                    node.start_position().row as u32 + 1,
                    node.end_position().row as u32 + 1,
                ),
            );
        }
    }

    fn finalize_classes(&mut self) -> Vec<ClassExtraction> {
        let groups = std::mem::take(&mut self.method_groups);
        let spans = std::mem::take(&mut self.struct_spans);

        groups
            .into_iter()
            .map(|(type_name, methods)| {
                let (byte_start, byte_end, start_line, end_line) =
                    if let Some(&span) = spans.get(&type_name) {
                        span
                    } else {
                        // No struct declaration found; use method span
                        let first = methods.first().unwrap();
                        let last = methods.last().unwrap();
                        (
                            first.byte_start,
                            last.byte_end,
                            first.start_line,
                            last.end_line,
                        )
                    };
                ClassExtraction {
                    name: type_name,
                    byte_start,
                    byte_end,
                    start_line,
                    end_line,
                    methods,
                }
            })
            .collect()
    }
}

/// Extract the type name from a receiver type, stripping pointer indirection.
fn extract_type_name(type_node: &Node, source: &str) -> String {
    match type_node.kind() {
        "pointer_type" => {
            // *MyStruct → MyStruct
            if let Some(inner) = type_node.named_child(0) {
                node_text_owned(&inner, source)
            } else {
                node_text_owned(type_node, source)
            }
        }
        "type_identifier" => node_text_owned(type_node, source),
        _ => node_text_owned(type_node, source),
    }
}

// ─── Parameter counting ──────────────────────────────────────────────────────

fn count_params(params_node: &Node, _source: &str) -> u32 {
    let mut count = 0u32;
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        if child.kind() == "parameter_declaration" {
            // A parameter_declaration can declare multiple names: `a, b int`
            let names = count_param_names(&child);
            count += if names > 0 { names } else { 1 };
        } else if child.kind() == "variadic_parameter_declaration" {
            count += 1;
        }
    }
    count
}

fn count_param_names(param: &Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = param.walk();
    for child in param.named_children(&mut cursor) {
        if child.kind() == "identifier" {
            count += 1;
        }
    }
    count
}

fn count_statements(block: &Node) -> u32 {
    // Go blocks are: block → { statement_list }
    let target = find_statement_list(block);
    count_statements_recursive(&target)
}

fn find_statement_list<'a>(block: &'a Node<'a>) -> Node<'a> {
    let mut cursor = block.walk();
    for child in block.named_children(&mut cursor) {
        if child.kind() == "statement_list" {
            return child;
        }
    }
    *block
}

/// Count all statements recursively, including those inside nested blocks.
fn count_statements_recursive(node: &Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        count += 1;
        count += count_nested_statements(&child);
    }
    count
}

/// Count statements inside nested blocks of a statement node.
fn count_nested_statements(node: &Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "block" => count += count_statements(&child),
            "statement_list" => count += count_statements_recursive(&child),
            "expression_case" | "default_case" | "type_case" | "communication_case" => {
                count += count_nested_statements(&child);
            }
            _ => {}
        }
    }
    count
}

// ─── Function body event emitter ────────────────────────────────────────────

struct GoBodyEventEmitter<'src> {
    events: Vec<QualitasEvent>,
    fn_name: String,
    source: &'src str,
    imported_names: &'src HashSet<String>,
    nesting_depth: u32,
    /// When true, skip CFC events (used for defer bodies).
    suppress_cfc: bool,
}

impl<'src> GoBodyEventEmitter<'src> {
    fn new(source: &'src str, fn_name: &str, imported_names: &'src HashSet<String>) -> Self {
        Self {
            events: Vec::with_capacity(256),
            fn_name: fn_name.to_string(),
            source,
            imported_names,
            nesting_depth: 0,
            suppress_cfc: false,
        }
    }

    // ── Block visitor ───────────────────────────────────────────────────

    fn visit_block(&mut self, node: &Node) {
        // Go blocks are: block → { statement_list }
        // We need to find statement_list inside blocks
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "statement_list" {
                let mut inner_cursor = child.walk();
                for stmt in child.named_children(&mut inner_cursor) {
                    self.visit_statement(&stmt);
                }
                return;
            }
        }
        // Fallback: if no statement_list found, walk children directly
        let mut cursor2 = node.walk();
        for child in node.named_children(&mut cursor2) {
            self.visit_statement(&child);
        }
    }

    fn visit_statement(&mut self, node: &Node) {
        match node.kind() {
            "if_statement" => self.visit_if(node),
            "for_statement" => self.visit_for(node),
            "expression_switch_statement" => self.visit_switch(node),
            "type_switch_statement" => self.visit_type_switch(node),
            "select_statement" => self.visit_select(node),
            "defer_statement" => self.visit_defer(node),
            "go_statement" => self.visit_go(node),
            "return_statement" => self.visit_return(node),
            "short_var_declaration" => self.visit_short_var_decl(node),
            "assignment_statement" => self.visit_assignment(node),
            "inc_statement" | "dec_statement" => self.visit_inc_dec(node),
            "expression_statement" => {
                if let Some(expr) = node.named_child(0) {
                    self.visit_expr(&expr);
                }
            }
            "var_declaration" => self.visit_var_decl(node),
            "send_statement" => self.visit_send(node),
            "labeled_statement" => self.visit_labeled(node),
            "block" => self.visit_block(node),
            "empty_statement"
            | "comment"
            | "const_declaration"
            | "type_declaration"
            | "break_statement"
            | "continue_statement"
            | "goto_statement"
            | "fallthrough_statement" => {}
            _ => {
                self.visit_children_exprs(node);
            }
        }
    }

    // ── Control flow ────────────────────────────────────────────────────

    fn visit_if(&mut self, node: &Node) {
        let has_else = node.child_by_field_name("alternative").is_some();
        let else_is_if = node
            .child_by_field_name("alternative")
            .is_some_and(|alt| alt.kind() == "if_statement");

        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind: ControlFlowKind::If,
                    has_else,
                    else_is_if,
                }));
        }

        // Initializer (e.g., `if err := foo(); err != nil`)
        if let Some(init) = node.child_by_field_name("initializer") {
            self.visit_statement(&init);
        }

        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }

        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;

        if let Some(body) = node.child_by_field_name("consequence") {
            self.visit_block(&body);
        }

        // else / else if
        if let Some(alt) = node.child_by_field_name("alternative") {
            if alt.kind() == "if_statement" {
                // else if — the nested if emits its own ControlFlow
                self.visit_if(&alt);
            } else {
                // else block
                self.visit_block(&alt);
            }
        }

        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_for(&mut self, node: &Node) {
        // Detect for variant from child nodes
        let has_range = Self::has_child_kind(node, "range_clause");
        let has_for_clause = Self::has_child_kind(node, "for_clause");

        let kind = if has_range {
            ControlFlowKind::ForOf
        } else if has_for_clause {
            ControlFlowKind::For
        } else {
            // Check if there's a condition expression (while-like)
            // or bare `for {}` (infinite loop → While)
            ControlFlowKind::While
        };

        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind,
                    has_else: false,
                    else_is_if: false,
                }));
        }

        // Visit for clause parts
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "for_clause" => self.visit_for_clause(&child),
                "range_clause" => self.visit_range_clause(&child),
                "block" => {} // handled below
                _ => {
                    // Bare condition: `for x > 0 {}`
                    self.visit_expr(&child);
                }
            }
        }

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block(&body);
            }
        });
    }

    fn visit_for_clause(&mut self, node: &Node) {
        if let Some(init) = node.child_by_field_name("initializer") {
            self.visit_statement(&init);
        }
        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }
        if let Some(update) = node.child_by_field_name("update") {
            self.visit_statement(&update);
        }
    }

    fn visit_range_clause(&mut self, node: &Node) {
        // `for k, v := range items`
        if let Some(left) = node.child_by_field_name("left") {
            self.emit_pattern_idents(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
    }

    fn visit_switch(&mut self, node: &Node) {
        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind: ControlFlowKind::Switch,
                    has_else: false,
                    else_is_if: false,
                }));
        }

        // Initializer and value
        if let Some(init) = node.child_by_field_name("initializer") {
            self.visit_statement(&init);
        }
        if let Some(value) = node.child_by_field_name("value") {
            self.visit_expr(&value);
        }

        self.emit_nesting_block(|s| {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "expression_case" => s.visit_switch_case(&child),
                    "default_case" => s.visit_default_case(&child),
                    _ => {}
                }
            }
        });
    }

    fn visit_type_switch(&mut self, node: &Node) {
        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind: ControlFlowKind::Switch,
                    has_else: false,
                    else_is_if: false,
                }));
        }

        // Visit the type switch value expression
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "type_switch_guard" {
                self.visit_children_exprs(&child);
            }
        }

        self.emit_nesting_block(|s| {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "type_case" => s.visit_type_case(&child),
                    "default_case" => s.visit_default_case(&child),
                    _ => {}
                }
            }
        });
    }

    fn visit_switch_case(&mut self, node: &Node) {
        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind: ControlFlowKind::ContextManager,
                    has_else: false,
                    else_is_if: false,
                }));
        }
        // Visit case value expressions
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "expression_list" {
                self.visit_children_exprs(&child);
            } else if child.kind() != "block" && child.kind() != "expression_list" {
                self.visit_statement(&child);
            }
        }
        // Visit case body statements (they're direct children, not in a block)
        let mut cursor2 = node.walk();
        for child in node.named_children(&mut cursor2) {
            if child.kind() != "expression_list" {
                self.visit_statement(&child);
            }
        }
    }

    fn visit_type_case(&mut self, node: &Node) {
        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind: ControlFlowKind::ContextManager,
                    has_else: false,
                    else_is_if: false,
                }));
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            // Skip type_list, visit body statements
            if child.kind() != "type_list" && child.kind() != "type_identifier" {
                self.visit_statement(&child);
            }
        }
    }

    fn visit_default_case(&mut self, node: &Node) {
        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind: ControlFlowKind::ContextManager,
                    has_else: false,
                    else_is_if: false,
                }));
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_statement(&child);
        }
    }

    fn visit_select(&mut self, node: &Node) {
        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind: ControlFlowKind::ContextManager,
                    has_else: false,
                    else_is_if: false,
                }));
        }

        self.emit_nesting_block(|s| {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "communication_case" => s.visit_comm_case(&child),
                    "default_case" => s.visit_default_case(&child),
                    _ => {}
                }
            }
        });
    }

    fn visit_comm_case(&mut self, node: &Node) {
        if !self.suppress_cfc {
            self.events
                .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                    kind: ControlFlowKind::ContextManager,
                    has_else: false,
                    else_is_if: false,
                }));
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "send_statement" => self.visit_send(&child),
                "receive_statement" => self.visit_children_exprs(&child),
                _ => self.visit_statement(&child),
            }
        }
    }

    // ── Defer & go ──────────────────────────────────────────────────────

    fn visit_defer(&mut self, node: &Node) {
        // Defer doesn't contribute to CFC, but its expression's operators/operands
        // are counted for DCI.
        let prev = self.suppress_cfc;
        self.suppress_cfc = true;
        if let Some(expr) = node.named_child(0) {
            self.visit_expr(&expr);
        }
        self.suppress_cfc = prev;
    }

    fn visit_go(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::AsyncComplexity(AsyncEvent::Spawn));
        if let Some(expr) = node.named_child(0) {
            self.visit_expr(&expr);
        }
    }

    // ── Return ──────────────────────────────────────────────────────────

    fn visit_return(&mut self, node: &Node) {
        self.events.push(QualitasEvent::ReturnStatement);
        if let Some(list) = node.child_by_field_name("result") {
            self.visit_expr(&list);
        } else {
            self.visit_children_exprs(node);
        }
    }

    // ── Assignment & declarations ───────────────────────────────────────

    fn visit_short_var_decl(&mut self, node: &Node) {
        self.emit_operator(":=");
        if let Some(left) = node.child_by_field_name("left") {
            self.emit_pattern_idents(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
    }

    fn visit_assignment(&mut self, node: &Node) {
        // Get the operator
        let mut cursor = node.walk();
        let mut op = "=".to_string();
        for child in node.children(&mut cursor) {
            if !child.is_named() {
                let text = node_text(&child, self.source);
                if text.contains('=') || text == "<<=" || text == ">>=" {
                    op = text.to_string();
                }
            }
        }
        self.emit_operator(&op);

        if let Some(left) = node.child_by_field_name("left") {
            self.visit_expr(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
    }

    fn visit_var_decl(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "var_spec" {
                self.visit_var_spec(&child);
            }
        }
    }

    fn visit_var_spec(&mut self, node: &Node) {
        // var x int = 5  or  var x, y = 1, 2
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    self.emit_ident_declaration(&child);
                }
                "expression_list" => {
                    self.emit_operator("=");
                    self.visit_children_exprs(&child);
                }
                _ => {}
            }
        }
        // Also check for a value field
        if let Some(value) = node.child_by_field_name("value") {
            if value.kind() != "expression_list" {
                self.emit_operator("=");
                self.visit_expr(&value);
            }
        }
    }

    fn visit_inc_dec(&mut self, node: &Node) {
        let op = if node.kind() == "inc_statement" {
            "++"
        } else {
            "--"
        };
        self.emit_operator(op);
        if let Some(expr) = node.named_child(0) {
            self.visit_expr(&expr);
        }
    }

    fn visit_send(&mut self, node: &Node) {
        self.emit_operator("<-");
        self.visit_children_exprs(node);
    }

    fn visit_labeled(&mut self, node: &Node) {
        self.events.push(QualitasEvent::LabeledFlow);
        // Visit the labeled statement
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() != "label_name" {
                self.visit_statement(&child);
            }
        }
    }

    // ── Expression visitor ──────────────────────────────────────────────

    fn visit_expr(&mut self, node: &Node) {
        match node.kind() {
            "binary_expression" => self.visit_binary_op(node),
            "unary_expression" => self.visit_unary_op(node),
            "call_expression" => self.visit_call(node),
            "selector_expression" => self.visit_selector(node),
            "identifier" => self.visit_identifier(node),
            "int_literal" | "float_literal" | "rune_literal" | "true" | "false" | "nil"
            | "iota" => self.visit_literal(node),
            "interpreted_string_literal" | "raw_string_literal" => {
                self.visit_string_literal(node);
            }
            "func_literal" => self.visit_func_literal(node),
            "composite_literal" => self.visit_composite_literal(node),
            "index_expression" => self.visit_index(node),
            "slice_expression" => self.visit_slice(node),
            "type_assertion_expression" => self.visit_type_assertion(node),
            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    self.visit_expr(&inner);
                }
            }
            "expression_list" => self.visit_children_exprs(node),
            "type_conversion_expression" => self.visit_type_conversion(node),
            _ => self.visit_expr_children(node),
        }
    }

    fn visit_expr_children(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_expr(&child);
        }
    }

    // ── Operators ───────────────────────────────────────────────────────

    fn visit_binary_op(&mut self, node: &Node) {
        let op = self.get_operator_text(node);
        match op.as_str() {
            "&&" => {
                self.events.push(QualitasEvent::LogicOp(LogicOpEvent::And));
                self.emit_operator(&op);
            }
            "||" => {
                self.events.push(QualitasEvent::LogicOp(LogicOpEvent::Or));
                self.emit_operator(&op);
            }
            _ => self.emit_operator(&op),
        }
        if let Some(left) = node.child_by_field_name("left") {
            self.visit_expr(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
    }

    fn visit_unary_op(&mut self, node: &Node) {
        let op = self.get_operator_text(node);
        // Receive operator `<-ch` is special
        if op == "<-" {
            self.emit_operator("<-");
        } else {
            self.emit_operator(&op);
        }
        if let Some(operand) = node.child_by_field_name("operand") {
            self.visit_expr(&operand);
        }
    }

    // ── Calls ───────────────────────────────────────────────────────────

    fn visit_call(&mut self, node: &Node) {
        let func_node = node.child_by_field_name("function");

        if let Some(ref f) = func_node {
            self.detect_call_patterns(f);
        }

        if let Some(f) = func_node {
            self.visit_expr(&f);
        }

        if let Some(args) = node.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            for arg in args.named_children(&mut cursor) {
                self.visit_expr(&arg);
            }
        }
    }

    fn detect_call_patterns(&mut self, func: &Node) {
        self.detect_recursive_call(func);
        self.detect_api_call(func);
    }

    fn detect_recursive_call(&mut self, func: &Node) {
        if func.kind() == "identifier" && node_text(func, self.source) == self.fn_name {
            self.events.push(QualitasEvent::RecursiveCall);
        }
    }

    fn detect_api_call(&mut self, func: &Node) {
        if func.kind() != "selector_expression" {
            return;
        }
        let Some(obj) = func.child_by_field_name("operand") else {
            return;
        };
        let Some(field) = func.child_by_field_name("field") else {
            return;
        };
        let obj_name = node_text(&obj, self.source);
        if obj.kind() == "identifier" && self.imported_names.contains(obj_name) {
            self.events.push(QualitasEvent::ApiCall(ApiCallEvent {
                object: obj_name.to_string(),
                method: node_text(&field, self.source).to_string(),
            }));
        }
    }

    fn visit_selector(&mut self, node: &Node) {
        if let Some(obj) = node.child_by_field_name("operand") {
            self.visit_expr(&obj);
        }
        if let Some(field) = node.child_by_field_name("field") {
            let name = node_text(&field, self.source);
            self.events.push(QualitasEvent::Operand(OperandEvent {
                name: name.to_string(),
            }));
            self.emit_operator(".");
        }
    }

    // ── Identifiers & literals ──────────────────────────────────────────

    fn visit_identifier(&mut self, node: &Node) {
        let name = node_text(node, self.source);
        self.events.push(QualitasEvent::Operand(OperandEvent {
            name: name.to_string(),
        }));
        self.events.push(QualitasEvent::IdentReference(IdentEvent {
            name: name.to_string(),
            byte_offset: node.start_byte() as u32,
        }));
    }

    fn visit_literal(&mut self, node: &Node) {
        let text = node_text(node, self.source);
        let name = match node.kind() {
            "true" => "true".to_string(),
            "false" => "false".to_string(),
            "nil" => "nil".to_string(),
            "iota" => "iota".to_string(),
            _ => text[..text.len().min(32)].to_string(),
        };
        self.events
            .push(QualitasEvent::Operand(OperandEvent { name }));
    }

    fn visit_string_literal(&mut self, node: &Node) {
        let text = node_text(node, self.source);
        let name = text[..text.len().min(32)].to_string();
        self.events
            .push(QualitasEvent::Operand(OperandEvent { name }));
    }

    // ── Func literal (closure) ──────────────────────────────────────────

    fn visit_func_literal(&mut self, node: &Node) {
        if self.nesting_depth > 0 {
            self.events.push(QualitasEvent::NestedCallback);
        }
        self.emit_nested_fn_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block(&body);
            }
        });
    }

    // ── Composite literal ───────────────────────────────────────────────

    fn visit_composite_literal(&mut self, node: &Node) {
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        self.visit_composite_elements(&body);
    }

    fn visit_composite_elements(&mut self, body: &Node) {
        let mut cursor = body.walk();
        for child in body.named_children(&mut cursor) {
            self.visit_composite_element(&child);
        }
    }

    fn visit_composite_element(&mut self, node: &Node) {
        match node.kind() {
            "keyed_element" => self.visit_keyed_element_value(node),
            "literal_element" => self.visit_literal_element_value(node),
            _ => self.visit_expr(node),
        }
    }

    fn visit_keyed_element_value(&mut self, node: &Node) {
        // keyed_element is `key: value`; we only visit the value side.
        let mut inner_cursor = node.walk();
        let mut value = None;
        for child in node.named_children(&mut inner_cursor) {
            value = Some(child);
        }
        if let Some(value) = value {
            self.visit_expr(&value);
        }
    }

    fn visit_literal_element_value(&mut self, node: &Node) {
        let Some(inner) = node.named_child(0) else {
            return;
        };
        self.visit_expr(&inner);
    }

    // ── Index & slice ───────────────────────────────────────────────────

    fn visit_index(&mut self, node: &Node) {
        self.emit_operator("[]");
        if let Some(operand) = node.child_by_field_name("operand") {
            self.visit_expr(&operand);
        }
        if let Some(index) = node.child_by_field_name("index") {
            self.visit_expr(&index);
        }
    }

    fn visit_slice(&mut self, node: &Node) {
        self.emit_operator(":");
        if let Some(operand) = node.child_by_field_name("operand") {
            self.visit_expr(&operand);
        }
        if let Some(start) = node.child_by_field_name("start") {
            self.visit_expr(&start);
        }
        if let Some(end) = node.child_by_field_name("end") {
            self.visit_expr(&end);
        }
        if let Some(capacity) = node.child_by_field_name("capacity") {
            self.visit_expr(&capacity);
        }
    }

    // ── Type assertion ──────────────────────────────────────────────────

    fn visit_type_assertion(&mut self, node: &Node) {
        self.emit_operator(".(type)");
        if let Some(operand) = node.child_by_field_name("operand") {
            self.visit_expr(&operand);
        }
    }

    // ── Type conversion ─────────────────────────────────────────────────

    fn visit_type_conversion(&mut self, node: &Node) {
        if let Some(operand) = node.child_by_field_name("operand") {
            self.visit_expr(&operand);
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    fn emit_operator(&mut self, name: &str) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: name.to_string(),
        }));
    }

    fn emit_nesting_block<F>(&mut self, body: F)
    where
        F: FnOnce(&mut Self),
    {
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        body(self);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn emit_nested_fn_block<F>(&mut self, body: F)
    where
        F: FnOnce(&mut Self),
    {
        self.events.push(QualitasEvent::NestedFunctionEnter);
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        body(self);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
        self.events.push(QualitasEvent::NestedFunctionExit);
    }

    fn emit_pattern_idents(&mut self, node: &Node) {
        match node.kind() {
            "identifier" => self.emit_ident_declaration(node),
            "expression_list" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.emit_pattern_idents(&child);
                }
            }
            _ => {}
        }
    }

    fn emit_ident_declaration(&mut self, node: &Node) {
        let name = node_text(node, self.source);
        if name == "_" {
            return; // blank identifier
        }
        self.events
            .push(QualitasEvent::IdentDeclaration(IdentEvent {
                name: name.to_string(),
                byte_offset: node.start_byte() as u32,
            }));
    }

    fn has_child_kind(node: &Node, kind: &str) -> bool {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == kind {
                return true;
            }
        }
        false
    }

    fn get_operator_text(&self, node: &Node) -> String {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !child.is_named() {
                let text = node_text(&child, self.source);
                if text != "(" && text != ")" && text != "," && text != "{" && text != "}" {
                    return text.to_string();
                }
            }
        }
        "?op".to_string()
    }

    fn visit_children_exprs(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_expr(&child);
        }
    }
}
