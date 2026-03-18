/// Python language adapter.
///
/// Uses `tree-sitter-python` for AST analysis and emits `QualitasEvent`s
/// for the language-agnostic metric collectors.
use std::collections::HashSet;

use tree_sitter::{Node, Parser};

use crate::ir::events::{
    ApiCallEvent, AsyncEvent, ControlFlowEvent, ControlFlowKind, IdentEvent, LogicOpEvent,
    OperandEvent, OperatorEvent, QualitasEvent,
};
use crate::ir::language::{
    ClassExtraction, FileExtraction, FunctionExtraction, ImportRecord, LanguageAdapter,
    ThresholdOverrides,
};

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn name(&self) -> &'static str {
        "Python"
    }

    fn extensions(&self) -> &[&str] {
        &[".py", ".pyi"]
    }

    fn test_patterns(&self) -> &[&str] {
        &["test_", "_test.py", "_tests.py", "tests/", "conftest.py"]
    }

    fn threshold_overrides(&self) -> Option<ThresholdOverrides> {
        Some(ThresholdOverrides {
            norm_sm_loc: Some(60.0),
            ..Default::default()
        })
    }

    fn extract(&self, source: &str, file_path: &str) -> Result<FileExtraction, String> {
        let mut parser = Parser::new();
        let language = tree_sitter_python::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| format!("qualitas: failed to set Python language: {e}"))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| format!("qualitas parse error for {file_path}: failed to parse"))?;

        let root = tree.root_node();
        if root.has_error() {
            return Err(format!(
                "qualitas parse error for {file_path}: syntax error"
            ));
        }

        let mut extractor = PythonExtractor::new(source);
        extractor.extract_top_level(&root);

        Ok(FileExtraction {
            functions: extractor.functions,
            classes: extractor.classes,
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

struct PythonExtractor<'src> {
    source: &'src str,
    functions: Vec<FunctionExtraction>,
    classes: Vec<ClassExtraction>,
    imports: Vec<ImportRecord>,
    imported_names: HashSet<String>,
}

impl<'src> PythonExtractor<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            functions: Vec::new(),
            classes: Vec::new(),
            imports: Vec::new(),
            imported_names: HashSet::new(),
        }
    }

    fn extract_top_level(&mut self, root: &Node) {
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "function_definition" | "async_function_definition" => {
                    if let Some(fe) = self.extract_function(&child) {
                        self.functions.push(fe);
                    }
                }
                "decorated_definition" => {
                    self.extract_decorated(&child, None);
                }
                "class_definition" => {
                    if let Some(ce) = self.extract_class(&child) {
                        self.classes.push(ce);
                    }
                }
                "import_statement" => self.extract_import(&child),
                "import_from_statement" => self.extract_import_from(&child),
                _ => {}
            }
        }
    }

    fn extract_decorated(
        &mut self,
        node: &Node,
        class_methods: Option<&mut Vec<FunctionExtraction>>,
    ) {
        // The actual definition is the last named child
        let Some(definition) = node.child_by_field_name("definition") else {
            return;
        };

        match definition.kind() {
            "function_definition" | "async_function_definition" => {
                self.handle_decorated_function(node, &definition, class_methods);
            }
            "class_definition" => {
                self.handle_decorated_class(node, &definition);
            }
            _ => {}
        }
    }

    fn handle_decorated_function(
        &mut self,
        decorator_node: &Node,
        definition: &Node,
        class_methods: Option<&mut Vec<FunctionExtraction>>,
    ) {
        if let Some(fe) = self.extract_function(definition) {
            let fe = FunctionExtraction {
                byte_start: decorator_node.start_byte() as u32,
                start_line: decorator_node.start_position().row as u32 + 1,
                ..fe
            };
            if let Some(methods) = class_methods {
                methods.push(fe);
            } else {
                self.functions.push(fe);
            }
        }
    }

    fn handle_decorated_class(&mut self, decorator_node: &Node, definition: &Node) {
        if let Some(ce) = self.extract_class(definition) {
            let ce = ClassExtraction {
                byte_start: decorator_node.start_byte() as u32,
                start_line: decorator_node.start_position().row as u32 + 1,
                ..ce
            };
            self.classes.push(ce);
        }
    }

    fn extract_function(&self, node: &Node) -> Option<FunctionExtraction> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text_owned(&name_node, self.source);
        let params_node = node.child_by_field_name("parameters");
        let param_count = params_node.map_or(0, |p| count_params(&p, self.source));
        let is_async = node.kind() == "async_function_definition";
        let body_node = node.child_by_field_name("body")?;

        let mut emitter = PythonBodyEventEmitter::new(self.source, &name, &self.imported_names);
        emitter.visit_block(&body_node);

        Some(FunctionExtraction {
            name,
            inferred_name: None,
            byte_start: node.start_byte() as u32,
            byte_end: node.end_byte() as u32,
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            param_count,
            is_async,
            is_generator: false,
            events: emitter.events,
            loc_override: None,
            statement_count: Some(count_statements(&body_node)),
        })
    }

    fn extract_class(&mut self, node: &Node) -> Option<ClassExtraction> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text_owned(&name_node, self.source);
        let body_node = node.child_by_field_name("body")?;

        let methods = self.extract_class_methods(&body_node);

        Some(ClassExtraction {
            name,
            byte_start: node.start_byte() as u32,
            byte_end: node.end_byte() as u32,
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            methods,
        })
    }

    fn extract_class_methods(&mut self, body_node: &Node) -> Vec<FunctionExtraction> {
        let mut methods = Vec::new();
        let mut cursor = body_node.walk();
        for child in body_node.named_children(&mut cursor) {
            match child.kind() {
                "function_definition" | "async_function_definition" => {
                    if let Some(mut fe) = self.extract_function(&child) {
                        fe.param_count = strip_self_param(&child, self.source, fe.param_count);
                        methods.push(fe);
                    }
                }
                "decorated_definition" => {
                    self.extract_decorated(&child, Some(&mut methods));
                }
                _ => {}
            }
        }
        methods
    }

    fn extract_import(&mut self, node: &Node) {
        // `import foo`, `import foo as bar`, `import foo.bar`
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "dotted_name" => self.handle_dotted_import(&child),
                "aliased_import" => self.handle_aliased_import(&child),
                _ => {}
            }
        }
    }

    fn handle_dotted_import(&mut self, node: &Node) {
        let full_name = node_text_owned(node, self.source);
        let binding = full_name
            .split('.')
            .next()
            .unwrap_or(&full_name)
            .to_string();
        self.imported_names.insert(binding.clone());
        self.imports.push(ImportRecord {
            source: full_name,
            is_external: true,
            names: vec![binding],
        });
    }

    fn handle_aliased_import(&mut self, child: &Node) {
        let name_node = child.child_by_field_name("name");
        let alias_node = child.child_by_field_name("alias");
        let source_name = name_node
            .map(|n| node_text_owned(&n, self.source))
            .unwrap_or_default();
        let binding = alias_node.map_or_else(
            || {
                source_name
                    .split('.')
                    .next()
                    .unwrap_or(&source_name)
                    .to_string()
            },
            |n| node_text_owned(&n, self.source),
        );
        self.imported_names.insert(binding.clone());
        self.imports.push(ImportRecord {
            source: source_name,
            is_external: true,
            names: vec![binding],
        });
    }

    fn extract_import_from(&mut self, node: &Node) {
        // `from foo import bar, baz`, `from foo import bar as b`
        let module = node
            .child_by_field_name("module_name")
            .map(|n| node_text_owned(&n, self.source))
            .unwrap_or_default();
        let is_external = !module.starts_with('.');
        let module_end = node
            .child_by_field_name("module_name")
            .map_or(0, |n| n.end_byte());

        let names = self.collect_import_from_names(node, module_end);

        for n in &names {
            self.imported_names.insert(n.clone());
        }

        self.imports.push(ImportRecord {
            source: module,
            is_external,
            names,
        });
    }

    fn collect_import_from_names(&self, node: &Node, module_end: usize) -> Vec<String> {
        let mut names = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "dotted_name" | "identifier" if child.start_byte() > module_end => {
                    names.push(node_text_owned(&child, self.source));
                }
                "aliased_import" => {
                    let binding = child
                        .child_by_field_name("alias")
                        .or_else(|| child.child_by_field_name("name"))
                        .map(|n| node_text_owned(&n, self.source))
                        .unwrap_or_default();
                    names.push(binding);
                }
                "wildcard_import" => names.push("*".to_string()),
                _ => {}
            }
        }
        names
    }
}

// ─── Parameter counting ──────────────────────────────────────────────────────

fn count_params(params_node: &Node, _source: &str) -> u32 {
    let mut count = 0u32;
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        match child.kind() {
            "identifier"
            | "typed_parameter"
            | "default_parameter"
            | "typed_default_parameter"
            | "list_splat_pattern"
            | "dictionary_splat_pattern" => {
                count += 1;
            }
            _ => {}
        }
    }
    count
}

fn strip_self_param(func_node: &Node, source: &str, param_count: u32) -> u32 {
    if let Some(params) = func_node.child_by_field_name("parameters") {
        if let Some(first) = params.named_child(0) {
            let text = node_text(&first, source);
            if text == "self" || text == "cls" {
                return param_count.saturating_sub(1);
            }
        }
    }
    param_count
}

fn count_statements(block: &Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = block.walk();
    for child in block.named_children(&mut cursor) {
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
            // else/elif/except/finally clauses contain a body block
            "elif_clause"
            | "else_clause"
            | "except_clause"
            | "except_group_clause"
            | "finally_clause"
            | "case_clause" => {
                count += count_nested_statements(&child);
            }
            "body" => {
                // Some nodes use a "body" field that is a block
                count += count_statements(&child);
            }
            _ => {}
        }
    }
    count
}

// ─── Function body event emitter ────────────────────────────────────────────

struct PythonBodyEventEmitter<'src> {
    events: Vec<QualitasEvent>,
    fn_name: String,
    source: &'src str,
    imported_names: &'src HashSet<String>,
    nesting_depth: u32,
}

impl<'src> PythonBodyEventEmitter<'src> {
    fn new(source: &'src str, fn_name: &str, imported_names: &'src HashSet<String>) -> Self {
        Self {
            events: Vec::with_capacity(256),
            fn_name: fn_name.to_string(),
            source,
            imported_names,
            nesting_depth: 0,
        }
    }

    // ── Block visitor ───────────────────────────────────────────────────

    fn visit_block(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_statement(&child);
        }
    }

    fn visit_statement(&mut self, node: &Node) {
        match node.kind() {
            "if_statement" => self.visit_if(node),
            "for_statement" | "async_for_statement" => self.visit_for(node),
            "while_statement" => self.visit_while(node),
            "with_statement" | "async_with_statement" => self.visit_with(node),
            "try_statement" => self.visit_try(node),
            "match_statement" => self.visit_match(node),
            "return_statement" => self.visit_return(node),
            "raise_statement" => self.visit_raise(node),
            "yield" => {
                self.events.push(QualitasEvent::ReturnStatement);
            }
            "expression_statement" => {
                if let Some(expr) = node.named_child(0) {
                    self.visit_expr(&expr);
                }
            }
            "assignment" => self.visit_assignment(node),
            "augmented_assignment" => self.visit_augmented_assignment(node),
            "function_definition" | "async_function_definition" => {
                self.visit_nested_function(node);
            }
            "decorated_definition" => self.handle_decorated_statement(node),
            "assert_statement" => self.visit_assert(node),
            "delete_statement" => {
                self.emit_operator("del");
                self.visit_children_exprs(node);
            }
            "global_statement" | "nonlocal_statement" | "pass_statement" | "break_statement"
            | "continue_statement" | "comment" | "class_definition" => {}
            _ => {
                self.visit_children_exprs(node);
            }
        }
    }

    fn handle_decorated_statement(&mut self, node: &Node) {
        if let Some(def) = node.child_by_field_name("definition") {
            match def.kind() {
                "function_definition" | "async_function_definition" => {
                    self.visit_nested_function(&def);
                }
                _ => {}
            }
        }
    }

    // ── Control flow ────────────────────────────────────────────────────

    fn visit_if(&mut self, node: &Node) {
        let has_else = Self::has_child_kind(node, "else_clause");
        let has_elif = Self::has_child_kind(node, "elif_clause");

        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::If,
                has_else: has_else || has_elif,
                else_is_if: has_elif,
            }));

        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }

        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;

        if let Some(body) = node.child_by_field_name("consequence") {
            self.visit_block(&body);
        }

        self.visit_if_clauses(node);

        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_if_clauses(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "elif_clause" => self.visit_elif(&child),
                "else_clause" => {
                    if let Some(body) = child.child_by_field_name("body") {
                        self.visit_block(&body);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_elif(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::LogicOp(LogicOpEvent::Ternary));
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::If,
                has_else: false,
                else_is_if: false,
            }));
        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }
        if let Some(body) = node.child_by_field_name("consequence") {
            self.visit_block(&body);
        }
    }

    fn visit_for(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ForOf,
                has_else: Self::has_child_kind(node, "else_clause"),
                else_is_if: false,
            }));

        // Loop variable bindings
        if let Some(left) = node.child_by_field_name("left") {
            self.emit_pattern_idents(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block(&body);
            }
        });

        // for...else
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "else_clause" {
                if let Some(body) = child.child_by_field_name("body") {
                    self.visit_block(&body);
                }
            }
        }
    }

    fn visit_while(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::While,
                has_else: Self::has_child_kind(node, "else_clause"),
                else_is_if: false,
            }));

        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block(&body);
            }
        });
    }

    fn visit_with(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ContextManager,
                has_else: false,
                else_is_if: false,
            }));

        self.visit_with_clauses(node);

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block(&body);
            }
        });
    }

    fn visit_with_clauses(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "with_clause" {
                self.visit_with_clause_items(&child);
            }
        }
    }

    fn visit_with_clause_items(&mut self, clause: &Node) {
        let mut cursor = clause.walk();
        for item in clause.named_children(&mut cursor) {
            if item.kind() == "with_item" {
                if let Some(value) = item.child_by_field_name("value") {
                    self.visit_expr(&value);
                }
                if let Some(alias) = item.child_by_field_name("alias") {
                    self.emit_pattern_idents(&alias);
                }
            }
        }
    }

    fn visit_try(&mut self, node: &Node) {
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_block(&body);
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_try_clause(&child);
        }
    }

    fn visit_try_clause(&mut self, child: &Node) {
        match child.kind() {
            "except_clause" | "except_group_clause" => {
                self.events
                    .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                        kind: ControlFlowKind::Catch,
                        has_else: false,
                        else_is_if: false,
                    }));
                self.emit_nesting_block(|s| {
                    if let Some(body) = child.child_by_field_name("body") {
                        s.visit_block(&body);
                    }
                });
            }
            "else_clause" | "finally_clause" => {
                if let Some(body) = child.child_by_field_name("body") {
                    self.visit_block(&body);
                }
            }
            _ => {}
        }
    }

    fn visit_match(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::PatternMatch,
                has_else: false,
                else_is_if: false,
            }));

        // Visit the subject expression
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() != "case_clause" && child.kind() != "block" {
                self.visit_expr(&child);
            }
        }

        self.emit_nesting_block(|s| {
            let mut cursor2 = node.walk();
            for child in node.named_children(&mut cursor2) {
                if child.kind() == "case_clause" {
                    s.visit_case_clause(&child);
                }
            }
        });
    }

    fn visit_case_clause(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ContextManager,
                has_else: false,
                else_is_if: false,
            }));
        // Visit guard if present
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "if_clause" {
                if let Some(cond) = child.named_child(0) {
                    self.visit_expr(&cond);
                }
            }
        }
        if let Some(body) = node.child_by_field_name("consequence") {
            self.visit_block(&body);
        }
    }

    // ── Return / raise / assert ──────────────────────────────────────────

    fn visit_return(&mut self, node: &Node) {
        self.events.push(QualitasEvent::ReturnStatement);
        if let Some(value) = node.named_child(0) {
            self.visit_expr(&value);
        }
    }

    fn visit_raise(&mut self, node: &Node) {
        self.events.push(QualitasEvent::ReturnStatement);
        self.emit_operator("raise");
        self.visit_children_exprs(node);
    }

    fn visit_assert(&mut self, node: &Node) {
        self.emit_operator("assert");
        self.visit_children_exprs(node);
    }

    // ── Assignment ──────────────────────────────────────────────────────

    fn visit_assignment(&mut self, node: &Node) {
        self.emit_operator("=");
        if let Some(left) = node.child_by_field_name("left") {
            self.emit_pattern_idents(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
        // Type annotation
        if let Some(ty) = node.child_by_field_name("type") {
            self.visit_expr(&ty);
        }
    }

    fn visit_augmented_assignment(&mut self, node: &Node) {
        // Get the operator (+=, -=, etc.)
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !child.is_named() {
                let op = node_text(&child, self.source);
                if op.ends_with('=') && op != "=" {
                    self.emit_operator(op);
                }
            }
        }
        if let Some(left) = node.child_by_field_name("left") {
            self.visit_expr(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
    }

    // ── Expression visitor ──────────────────────────────────────────────

    fn visit_expr(&mut self, node: &Node) {
        match node.kind() {
            "binary_operator" => self.visit_binary_op(node),
            "unary_operator" => self.visit_unary_op(node),
            "boolean_operator" => self.visit_boolean_op(node),
            "not_operator" => self.visit_not_op(node),
            "comparison_operator" => self.visit_comparison(node),
            "conditional_expression" => self.visit_conditional_expr(node),
            "call" => self.visit_call(node),
            "attribute" => self.visit_attribute(node),
            "identifier" => self.visit_identifier(node),
            "integer" | "float" | "true" | "false" | "none" => self.visit_literal(node),
            "string" | "concatenated_string" => self.visit_string_literal(node),
            "lambda" => self.visit_lambda(node),
            "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression" => self.visit_comprehension(node),
            "parenthesized_expression" => self.visit_parenthesized_expr(node),
            "tuple" | "list" | "set" => self.visit_collection(node),
            "dictionary" => self.visit_dict(node),
            "subscript" => self.visit_subscript(node),
            "slice" => self.visit_slice(node),
            "await" => self.visit_await(node),
            "yield" => self.visit_yield_expr(node),
            "assignment" => self.visit_assignment(node),
            "augmented_assignment" => self.visit_augmented_assignment(node),
            "named_expression" => self.visit_walrus(node),
            "starred_expression" => self.visit_starred_expr(node),
            "type" => {}
            "keyword_argument" | "pair" => self.visit_keyword_or_pair(node),
            "if_statement" | "for_statement" | "async_for_statement" | "while_statement" => {
                self.visit_statement(node);
            }
            "expression_list" => self.visit_collection(node),
            _ => self.visit_expr_children(node),
        }
    }

    fn visit_parenthesized_expr(&mut self, node: &Node) {
        if let Some(inner) = node.named_child(0) {
            self.visit_expr(&inner);
        }
    }

    fn visit_yield_expr(&mut self, node: &Node) {
        self.events.push(QualitasEvent::ReturnStatement);
        if let Some(val) = node.named_child(0) {
            self.visit_expr(&val);
        }
    }

    fn visit_starred_expr(&mut self, node: &Node) {
        self.emit_operator("*");
        if let Some(val) = node.named_child(0) {
            self.visit_expr(&val);
        }
    }

    fn visit_keyword_or_pair(&mut self, node: &Node) {
        if let Some(val) = node.child_by_field_name("value") {
            self.visit_expr(&val);
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
        self.emit_operator(&op);
        if let Some(left) = node.child_by_field_name("left") {
            self.visit_expr(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
    }

    fn visit_unary_op(&mut self, node: &Node) {
        let op = self.get_operator_text(node);
        self.emit_operator(&op);
        if let Some(operand) = node.child_by_field_name("argument") {
            self.visit_expr(&operand);
        }
    }

    fn visit_boolean_op(&mut self, node: &Node) {
        let op_text = self.get_operator_text(node);
        match op_text.as_str() {
            "and" => self.events.push(QualitasEvent::LogicOp(LogicOpEvent::And)),
            "or" => self.events.push(QualitasEvent::LogicOp(LogicOpEvent::Or)),
            _ => {}
        }
        self.emit_operator(&op_text);
        if let Some(left) = node.child_by_field_name("left") {
            self.visit_expr(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
    }

    fn visit_not_op(&mut self, node: &Node) {
        self.emit_operator("not");
        if let Some(arg) = node.child_by_field_name("argument") {
            self.visit_expr(&arg);
        }
    }

    fn visit_comparison(&mut self, node: &Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        if let Some(first) = children.first() {
            if first.is_named() {
                self.visit_expr(first);
            }
        }

        self.visit_comparison_chain(&children[1..]);
    }

    fn visit_comparison_chain(&mut self, children: &[Node]) {
        let mut i = 0;
        while i < children.len() {
            let child = &children[i];
            if child.is_named() {
                self.visit_expr(child);
                i += 1;
            } else {
                i += self.emit_comparison_op(children, i);
            }
        }
    }

    /// Emit a comparison operator, handling multi-token ops like "not in" and "is not".
    /// Returns the number of tokens consumed.
    fn emit_comparison_op(&mut self, children: &[Node], i: usize) -> usize {
        let op_text = node_text(&children[i], self.source);
        if (op_text == "not" || op_text == "is") && i + 1 < children.len() {
            let next = &children[i + 1];
            if !next.is_named() {
                self.emit_operator(&format!("{} {}", op_text, node_text(next, self.source)));
                return 2;
            }
        }
        self.emit_operator(op_text);
        1
    }

    fn visit_conditional_expr(&mut self, node: &Node) {
        // Python ternary: value_if_true if condition else value_if_false
        self.events
            .push(QualitasEvent::LogicOp(LogicOpEvent::Ternary));
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_expr(&child);
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
        self.detect_async_spawn(func);
    }

    fn detect_recursive_call(&mut self, func: &Node) {
        if func.kind() == "identifier" && node_text(func, self.source) == self.fn_name {
            self.events.push(QualitasEvent::RecursiveCall);
        }
    }

    fn detect_api_call(&mut self, func: &Node) {
        if func.kind() != "attribute" {
            return;
        }
        let Some(obj) = func.child_by_field_name("object") else {
            return;
        };
        let Some(attr) = func.child_by_field_name("attribute") else {
            return;
        };
        let obj_name = node_text(&obj, self.source);
        if obj.kind() == "identifier" && self.imported_names.contains(obj_name) {
            self.events.push(QualitasEvent::ApiCall(ApiCallEvent {
                object: obj_name.to_string(),
                method: node_text(&attr, self.source).to_string(),
            }));
        }
    }

    fn detect_async_spawn(&mut self, func: &Node) {
        let call_name = self.get_call_name(func);
        if matches!(
            call_name.as_str(),
            "asyncio.create_task" | "asyncio.ensure_future" | "asyncio.gather" | "loop.create_task"
        ) {
            self.events
                .push(QualitasEvent::AsyncComplexity(AsyncEvent::Spawn));
        }
    }

    fn visit_attribute(&mut self, node: &Node) {
        if let Some(obj) = node.child_by_field_name("object") {
            self.visit_expr(&obj);
        }
        if let Some(attr) = node.child_by_field_name("attribute") {
            let name = node_text(&attr, self.source);
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
            "true" => "True".to_string(),
            "false" => "False".to_string(),
            "none" => "None".to_string(),
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

    // ── Lambda ──────────────────────────────────────────────────────────

    fn visit_lambda(&mut self, node: &Node) {
        if self.nesting_depth > 0 {
            self.events.push(QualitasEvent::NestedCallback);
        }
        self.emit_nested_fn_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_expr(&body);
            }
        });
    }

    // ── Comprehensions ──────────────────────────────────────────────────

    fn visit_comprehension(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "for_in_clause" => self.visit_comprehension_for(&child),
                "if_clause" => self.visit_comprehension_if(&child),
                _ => self.visit_expr(&child),
            }
        }
    }

    fn visit_comprehension_for(&mut self, clause: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ForOf,
                has_else: false,
                else_is_if: false,
            }));
        let mut cursor = clause.walk();
        for inner in clause.named_children(&mut cursor) {
            if !matches!(
                inner.kind(),
                "identifier" | "pattern_list" | "tuple_pattern"
            ) {
                self.visit_expr(&inner);
            }
        }
    }

    fn visit_comprehension_if(&mut self, clause: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::If,
                has_else: false,
                else_is_if: false,
            }));
        let mut cursor = clause.walk();
        for inner in clause.named_children(&mut cursor) {
            self.visit_expr(&inner);
        }
    }

    // ── Collections ─────────────────────────────────────────────────────

    fn visit_collection(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_expr(&child);
        }
    }

    fn visit_dict(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "pair" {
                if let Some(key) = child.child_by_field_name("key") {
                    self.visit_expr(&key);
                }
                if let Some(value) = child.child_by_field_name("value") {
                    self.visit_expr(&value);
                }
            } else {
                self.visit_expr(&child);
            }
        }
    }

    fn visit_subscript(&mut self, node: &Node) {
        self.emit_operator("[]");
        if let Some(value) = node.child_by_field_name("value") {
            self.visit_expr(&value);
        }
        if let Some(subscript) = node.child_by_field_name("subscript") {
            self.visit_expr(&subscript);
        }
    }

    fn visit_slice(&mut self, node: &Node) {
        self.emit_operator(":");
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_expr(&child);
        }
    }

    // ── Await ───────────────────────────────────────────────────────────

    fn visit_await(&mut self, node: &Node) {
        if self.nesting_depth > 1 {
            self.events
                .push(QualitasEvent::AsyncComplexity(AsyncEvent::Await));
        }
        if let Some(val) = node.named_child(0) {
            self.visit_expr(&val);
        }
    }

    // ── Walrus operator := ──────────────────────────────────────────────

    fn visit_walrus(&mut self, node: &Node) {
        self.emit_operator(":=");
        if let Some(name) = node.child_by_field_name("name") {
            self.emit_ident_declaration(&name);
        }
        if let Some(value) = node.child_by_field_name("value") {
            self.visit_expr(&value);
        }
    }

    // ── Nested function ─────────────────────────────────────────────────

    fn visit_nested_function(&mut self, node: &Node) {
        self.emit_nested_fn_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block(&body);
            }
        });
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
            "pattern_list" | "tuple_pattern" | "list_pattern" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.emit_pattern_idents(&child);
                }
            }
            "starred_expression" => {
                if let Some(val) = node.named_child(0) {
                    self.emit_pattern_idents(&val);
                }
            }
            _ => {}
        }
    }

    fn emit_ident_declaration(&mut self, node: &Node) {
        let name = node_text(node, self.source);
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
                // Skip parentheses and other delimiters
                if text != "(" && text != ")" && text != "," {
                    return text.to_string();
                }
            }
        }
        "?op".to_string()
    }

    fn get_call_name(&self, func_node: &Node) -> String {
        match func_node.kind() {
            "identifier" => node_text(func_node, self.source).to_string(),
            "attribute" => {
                let obj = func_node
                    .child_by_field_name("object")
                    .map_or("", |n| node_text(&n, self.source));
                let attr = func_node
                    .child_by_field_name("attribute")
                    .map_or("", |n| node_text(&n, self.source));
                format!("{obj}.{attr}")
            }
            _ => String::new(),
        }
    }

    fn visit_children_exprs(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_expr(&child);
        }
    }
}
