/// Java language adapter.
///
/// Uses `tree-sitter-java` for AST analysis and emits `QualitasEvent`s
/// for the language-agnostic metric collectors.
use std::collections::HashSet;

use tree_sitter::{Node, Parser};

use crate::ir::events::{
    ApiCallEvent, ControlFlowEvent, ControlFlowKind, IdentEvent, LogicOpEvent, OperandEvent,
    OperatorEvent, QualitasEvent,
};
use crate::ir::language::{
    ClassExtraction, FileExtraction, FunctionExtraction, ImportRecord, LanguageAdapter,
};

pub struct JavaAdapter;

impl LanguageAdapter for JavaAdapter {
    fn name(&self) -> &'static str {
        "Java"
    }

    fn extensions(&self) -> &[&str] {
        &[".java"]
    }

    fn test_patterns(&self) -> &[&str] {
        &[
            "Test.java",
            "Tests.java",
            "test/",
            "test\\",
            "src/test/",
            "src\\test\\",
        ]
    }

    fn extract(&self, source: &str, file_path: &str) -> Result<FileExtraction, String> {
        let mut parser = Parser::new();
        let language = tree_sitter_java::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| format!("qualitas: failed to set Java language: {e}"))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| format!("qualitas parse error for {file_path}: failed to parse"))?;

        let root = tree.root_node();
        if root.has_error() {
            return Err(format!(
                "qualitas parse error for {file_path}: syntax error"
            ));
        }

        let mut extractor = JavaExtractor::new(source);
        extractor.extract_top_level(&root);

        Ok(FileExtraction {
            functions: Vec::new(),
            classes: extractor.classes,
            imports: extractor.imports,
            file_scope: None,
        })
    }
}

// ─── Top-level extraction ────────────────────────────────────────────────────

struct JavaExtractor<'src> {
    source: &'src str,
    classes: Vec<ClassExtraction>,
    imports: Vec<ImportRecord>,
    imported_names: HashSet<String>,
}

impl<'src> JavaExtractor<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            classes: Vec::new(),
            imports: Vec::new(),
            imported_names: HashSet::new(),
        }
    }

    fn extract_top_level(&mut self, root: &Node) {
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "import_declaration" => self.extract_import(&child),
                "class_declaration" | "interface_declaration" | "enum_declaration" => {
                    self.extract_class(&child, None);
                }
                _ => {}
            }
        }
    }

    fn extract_class(&mut self, node: &Node, outer_name: Option<&str>) {
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let simple_name = node_text_owned(&name_node, self.source);
        let class_name = match outer_name {
            Some(outer) => format!("{outer}.{simple_name}"),
            None => simple_name,
        };

        let Some(body_node) = node.child_by_field_name("body") else {
            return;
        };

        let methods = self.extract_class_body_members(&body_node, &class_name);

        self.classes.push(ClassExtraction {
            name: class_name,
            byte_start: node.start_byte() as u32,
            byte_end: node.end_byte() as u32,
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            methods,
        });
    }

    fn extract_class_body_members(
        &mut self,
        body_node: &Node,
        class_name: &str,
    ) -> Vec<FunctionExtraction> {
        let mut methods = Vec::new();
        let mut cursor = body_node.walk();
        for child in body_node.named_children(&mut cursor) {
            self.extract_class_member(&child, class_name, &mut methods);
        }
        methods
    }

    fn extract_class_member(
        &mut self,
        child: &Node,
        class_name: &str,
        methods: &mut Vec<FunctionExtraction>,
    ) {
        match child.kind() {
            "method_declaration" => {
                if let Some(fe) = self.extract_method(child, Some(class_name)) {
                    methods.push(fe);
                }
            }
            "constructor_declaration" => {
                if let Some(fe) = self.extract_constructor(child, class_name) {
                    methods.push(fe);
                }
            }
            "class_declaration" | "interface_declaration" | "enum_declaration" => {
                self.extract_class(child, Some(class_name));
            }
            "field_declaration" => {
                self.extract_anonymous_classes_from_field(child, class_name);
            }
            _ => {}
        }
    }

    fn extract_method(
        &mut self,
        node: &Node,
        parent_class: Option<&str>,
    ) -> Option<FunctionExtraction> {
        let name_node = node.child_by_field_name("name")?;
        let name = node_text_owned(&name_node, self.source);
        let params_node = node.child_by_field_name("parameters");
        let param_count = params_node.map_or(0, |p| count_params(&p));
        let body_node = node.child_by_field_name("body")?;

        // Scan method body for anonymous classes before emitting events
        if let Some(class_name) = parent_class {
            self.scan_for_anonymous_classes(&body_node, class_name);
        }

        let mut emitter = JavaBodyEventEmitter::new(self.source, &name, &self.imported_names);
        emitter.visit_block(&body_node);

        Some(build_function_extraction(
            node,
            &body_node,
            FunctionBuildInfo {
                name,
                param_count,
                events: emitter.events,
            },
        ))
    }

    fn scan_for_anonymous_classes(&mut self, node: &Node, parent_class: &str) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "object_creation_expression" {
                self.check_anonymous_class(&child, parent_class);
            }
            // Recurse into all children to find deeply nested anonymous classes
            self.scan_for_anonymous_classes(&child, parent_class);
        }
    }

    fn extract_constructor(&self, node: &Node, class_name: &str) -> Option<FunctionExtraction> {
        let params_node = node.child_by_field_name("parameters");
        let param_count = params_node.map_or(0, |p| count_params(&p));
        let body_node = node.child_by_field_name("body")?;

        // Use the simple class name (last segment) as the constructor name
        let name = class_name
            .rsplit('.')
            .next()
            .unwrap_or(class_name)
            .to_string();

        let mut emitter = JavaBodyEventEmitter::new(self.source, &name, &self.imported_names);
        emitter.visit_block(&body_node);

        Some(build_function_extraction(
            node,
            &body_node,
            FunctionBuildInfo {
                name,
                param_count,
                events: emitter.events,
            },
        ))
    }

    fn extract_anonymous_classes_from_field(&mut self, node: &Node, parent_class: &str) {
        // Look for field_declaration → variable_declarator → object_creation_expression with class_body
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                if let Some(value) = child.child_by_field_name("value") {
                    self.check_anonymous_class(&value, parent_class);
                }
            }
        }
    }

    fn check_anonymous_class(&mut self, node: &Node, parent_class: &str) {
        if node.kind() != "object_creation_expression" {
            return;
        }
        let Some(class_body) = find_child_by_kind(node, "class_body") else {
            return;
        };
        let type_name = self.anonymous_class_type_name(node);
        let anon_name = format!("{parent_class}.{type_name} (anonymous)");
        self.extract_anonymous_class_methods(&class_body, node, &anon_name);
    }

    fn anonymous_class_type_name(&self, node: &Node) -> String {
        if let Some(type_node) = node.child_by_field_name("type") {
            let text = node_text(&type_node, self.source);
            // Strip generic parameters: Comparator<String> → Comparator
            text.split('<').next().unwrap_or(text).trim().to_string()
        } else {
            "Anonymous".to_string()
        }
    }

    fn extract_anonymous_class_methods(
        &mut self,
        class_body: &Node,
        outer_node: &Node,
        class_name: &str,
    ) {
        let mut methods = Vec::new();
        let mut cursor = class_body.walk();
        for child in class_body.named_children(&mut cursor) {
            if child.kind() == "method_declaration" {
                if let Some(fe) = self.extract_method(&child, None) {
                    methods.push(fe);
                }
            }
        }
        if !methods.is_empty() {
            self.classes.push(ClassExtraction {
                name: class_name.to_string(),
                byte_start: outer_node.start_byte() as u32,
                byte_end: outer_node.end_byte() as u32,
                start_line: outer_node.start_position().row as u32 + 1,
                end_line: outer_node.end_position().row as u32 + 1,
                methods,
            });
        }
    }

    fn extract_import(&mut self, node: &Node) {
        // import java.util.List;         → binding "List"
        // import java.util.*;            → binding "util"
        // import static java.lang.Math.abs; → binding "abs"
        let text = node_text(node, self.source).trim().to_string();

        // Strip `import `, optional `static `, and trailing `;`
        let stripped = text
            .strip_prefix("import ")
            .unwrap_or(&text)
            .trim_start_matches("static ")
            .trim_end_matches(';')
            .trim();

        let source = stripped.to_string();

        let binding = if stripped.ends_with(".*") {
            // Wildcard: import java.util.* → "util"
            let without_star = stripped.trim_end_matches(".*");
            without_star
                .rsplit('.')
                .next()
                .unwrap_or(without_star)
                .to_string()
        } else {
            // Regular: import java.util.List → "List"
            stripped.rsplit('.').next().unwrap_or(stripped).to_string()
        };

        self.imported_names.insert(binding.clone());
        self.imports.push(ImportRecord {
            source,
            is_external: true,
            names: vec![binding],
        });
    }
}

// ─── Shared helpers ──────────────────────────────────────────────────────────

struct FunctionBuildInfo {
    name: String,
    param_count: u32,
    events: Vec<QualitasEvent>,
}

fn build_function_extraction(
    node: &Node,
    body_node: &Node,
    info: FunctionBuildInfo,
) -> FunctionExtraction {
    FunctionExtraction {
        name: info.name,
        inferred_name: None,
        byte_start: node.start_byte() as u32,
        byte_end: node.end_byte() as u32,
        start_line: node.start_position().row as u32 + 1,
        end_line: node.end_position().row as u32 + 1,
        param_count: info.param_count,
        is_async: false,
        is_generator: false,
        events: info.events,
        loc_override: None,
        statement_count: Some(count_statements(body_node)),
    }
}

fn count_params(params_node: &Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        if child.kind() == "formal_parameter" || child.kind() == "spread_parameter" {
            count += 1;
        }
    }
    count
}

fn count_statements(block: &Node) -> u32 {
    // Java blocks contain statements as direct named children (no statement_list wrapper)
    count_statements_recursive(block)
}

fn count_statements_recursive(node: &Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            // Skip non-statement children
            "comment"
            | "modifiers"
            | "type_identifier"
            | "void_type"
            | "integral_type"
            | "boolean_type"
            | "floating_point_type"
            | "generic_type"
            | "array_type"
            | "scoped_type_identifier"
            | "identifier"
            | "formal_parameters"
            | "formal_parameter"
            | "spread_parameter"
            | "throws"
            | "superclass"
            | "super_interfaces"
            | "type_parameters"
            | "annotation"
            | "marker_annotation"
            | "dimensions" => {}

            // Actual statements → count + recurse into nested blocks
            _ => {
                if is_statement_kind(child.kind()) {
                    count += 1;
                    count += count_nested_in_statement(&child);
                }
            }
        }
    }
    count
}

fn is_statement_kind(kind: &str) -> bool {
    matches!(
        kind,
        "local_variable_declaration"
            | "expression_statement"
            | "return_statement"
            | "throw_statement"
            | "if_statement"
            | "for_statement"
            | "enhanced_for_statement"
            | "while_statement"
            | "do_statement"
            | "switch_expression"
            | "try_statement"
            | "try_with_resources_statement"
            | "break_statement"
            | "continue_statement"
            | "assert_statement"
            | "labeled_statement"
            | "synchronized_statement"
            | "block"
    )
}

fn count_nested_in_statement(node: &Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        count += count_nested_child(&child);
    }
    count
}

fn count_nested_child(child: &Node) -> u32 {
    match child.kind() {
        "block" | "constructor_body" => count_statements_recursive(child),
        "switch_block" => count_statements_in_switch_block(child),
        "catch_clause" | "finally_clause" => count_nested_in_statement(child),
        "if_statement" => 1 + count_nested_in_statement(child),
        _ => 0,
    }
}

fn count_statements_in_switch_block(node: &Node) -> u32 {
    let mut count = 0u32;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "switch_block_statement_group" {
            count += count_statements_in_case_group(&child);
        }
    }
    count
}

fn count_statements_in_case_group(group: &Node) -> u32 {
    let mut count = 0u32;
    let mut inner = group.walk();
    for stmt in group.named_children(&mut inner) {
        if stmt.kind() != "switch_label" && is_statement_kind(stmt.kind()) {
            count += 1;
            count += count_nested_in_statement(&stmt);
        }
    }
    count
}

#[allow(clippy::manual_find)]
fn find_child_by_kind<'a>(node: &'a Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
    }
    None
}

fn node_text<'src>(node: &Node, source: &'src str) -> &'src str {
    &source[node.start_byte()..node.end_byte()]
}

fn node_text_owned(node: &Node, source: &str) -> String {
    node_text(node, source).to_string()
}

// ─── Function body event emitter ─────────────────────────────────────────────

struct JavaBodyEventEmitter<'src> {
    events: Vec<QualitasEvent>,
    fn_name: String,
    source: &'src str,
    imported_names: &'src HashSet<String>,
    nesting_depth: u32,
}

impl<'src> JavaBodyEventEmitter<'src> {
    fn new(source: &'src str, fn_name: &str, imported_names: &'src HashSet<String>) -> Self {
        Self {
            events: Vec::with_capacity(256),
            fn_name: fn_name.to_string(),
            source,
            imported_names,
            nesting_depth: 0,
        }
    }

    // ── Block visitor ────────────────────────────────────────────────────

    fn visit_block(&mut self, node: &Node) {
        // Java blocks contain statements as direct named children
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_statement(&child);
        }
    }

    /// Visit a node that may be a block or a single statement.
    /// Java allows `if (cond) return x;` without braces.
    fn visit_block_or_statement(&mut self, node: &Node) {
        if node.kind() == "block" {
            self.visit_block(node);
        } else {
            self.visit_statement(node);
        }
    }

    fn visit_statement(&mut self, node: &Node) {
        match node.kind() {
            "if_statement" => self.visit_if(node),
            "for_statement" => self.visit_for(node),
            "enhanced_for_statement" => self.visit_enhanced_for(node),
            "while_statement" => self.visit_while(node),
            "do_statement" => self.visit_do_while(node),
            "switch_expression" => self.visit_switch(node),
            "try_statement" => self.visit_try(node),
            "try_with_resources_statement" => self.visit_try_with_resources(node),
            "return_statement" => self.visit_return(node),
            "throw_statement" => self.visit_throw(node),
            "local_variable_declaration" => self.visit_local_var_decl(node),
            "expression_statement" => {
                if let Some(expr) = node.named_child(0) {
                    self.visit_expr(&expr);
                }
            }
            "labeled_statement" => self.visit_labeled(node),
            "break_statement" => self.visit_break_continue(node),
            "continue_statement" => self.visit_break_continue(node),
            "block" => self.visit_block(node),
            "synchronized_statement" => {
                // Visit the lock expression + body
                self.visit_children_exprs_and_blocks(node);
            }
            "assert_statement" | "empty_statement" | "comment" => {}
            _ => {}
        }
    }

    // ── Control flow ─────────────────────────────────────────────────────

    fn visit_if(&mut self, node: &Node) {
        let has_else = node.child_by_field_name("alternative").is_some();
        let else_is_if = node
            .child_by_field_name("alternative")
            .is_some_and(|alt| alt.kind() == "if_statement");

        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::If,
                has_else,
                else_is_if,
            }));

        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }

        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;

        if let Some(body) = node.child_by_field_name("consequence") {
            self.visit_block_or_statement(&body);
        }

        if let Some(alt) = node.child_by_field_name("alternative") {
            if alt.kind() == "if_statement" {
                self.visit_if(&alt);
            } else {
                self.visit_block_or_statement(&alt);
            }
        }

        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_for(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::For,
                has_else: false,
                else_is_if: false,
            }));

        // Visit init, condition, update expressions
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "block" => {}
                "local_variable_declaration" => self.visit_local_var_decl(&child),
                _ => self.visit_expr(&child),
            }
        }

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block_or_statement(&body);
            }
        });
    }

    fn visit_enhanced_for(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ForOf,
                has_else: false,
                else_is_if: false,
            }));

        // Declare the iteration variable
        if let Some(name_node) = node.child_by_field_name("name") {
            self.emit_ident_declaration(&name_node);
        }
        // Visit the iterable
        if let Some(value) = node.child_by_field_name("value") {
            self.visit_expr(&value);
        }

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block_or_statement(&body);
            }
        });
    }

    fn visit_while(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::While,
                has_else: false,
                else_is_if: false,
            }));

        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block_or_statement(&body);
            }
        });
    }

    fn visit_do_while(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::DoWhile,
                has_else: false,
                else_is_if: false,
            }));

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block_or_statement(&body);
            }
        });

        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }
    }

    fn visit_switch(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::Switch,
                has_else: false,
                else_is_if: false,
            }));

        // Visit the switch expression
        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_switch_block(&body);
            }
        });
    }

    fn visit_switch_block(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "switch_block_statement_group" {
                self.visit_switch_case_group(&child);
            }
        }
    }

    fn visit_switch_case_group(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ContextManager,
                has_else: false,
                else_is_if: false,
            }));
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() != "switch_label" {
                self.visit_statement(&child);
            }
        }
    }

    fn visit_try(&mut self, node: &Node) {
        // Visit the try body (no ControlFlow for try itself)
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_block(&body);
        }

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "catch_clause" => self.visit_catch(&child),
                "finally_clause" => self.visit_finally(&child),
                _ => {}
            }
        }
    }

    fn visit_try_with_resources(&mut self, node: &Node) {
        // Emit ContextManager for the resources
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ContextManager,
                has_else: false,
                else_is_if: false,
            }));

        // Visit resource declarations
        if let Some(resources) = node.child_by_field_name("resources") {
            let mut cursor = resources.walk();
            for child in resources.named_children(&mut cursor) {
                if child.kind() == "resource" {
                    self.visit_resource(&child);
                }
            }
        }

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block_or_statement(&body);
            }
        });

        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "catch_clause" => self.visit_catch(&child),
                "finally_clause" => self.visit_finally(&child),
                _ => {}
            }
        }
    }

    fn visit_resource(&mut self, node: &Node) {
        if let Some(name_node) = node.child_by_field_name("name") {
            self.emit_ident_declaration(&name_node);
        }
        if let Some(value) = node.child_by_field_name("value") {
            self.emit_operator("=");
            self.visit_expr(&value);
        }
    }

    fn visit_catch(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::Catch,
                has_else: false,
                else_is_if: false,
            }));

        self.emit_nesting_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                s.visit_block_or_statement(&body);
            }
        });
    }

    fn visit_finally(&mut self, node: &Node) {
        // Just visit the body; no ControlFlow event for finally
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "block" {
                self.visit_block(&child);
            }
        }
    }

    fn visit_labeled(&mut self, node: &Node) {
        self.events.push(QualitasEvent::LabeledFlow);
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() != "identifier" {
                self.visit_statement(&child);
            }
        }
    }

    fn visit_break_continue(&mut self, node: &Node) {
        // If there's a label identifier, emit LabeledFlow
        if let Some(_label) = node.named_child(0) {
            self.events.push(QualitasEvent::LabeledFlow);
        }
    }

    // ── Return / throw ───────────────────────────────────────────────────

    fn visit_return(&mut self, node: &Node) {
        self.events.push(QualitasEvent::ReturnStatement);
        self.visit_children_exprs(node);
    }

    fn visit_throw(&mut self, node: &Node) {
        self.events.push(QualitasEvent::ReturnStatement);
        self.visit_children_exprs(node);
    }

    // ── Declarations ─────────────────────────────────────────────────────

    fn visit_local_var_decl(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "variable_declarator" {
                self.visit_variable_declarator(&child);
            }
        }
    }

    fn visit_variable_declarator(&mut self, node: &Node) {
        if let Some(name_node) = node.child_by_field_name("name") {
            self.emit_ident_declaration(&name_node);
        }
        if let Some(value) = node.child_by_field_name("value") {
            self.emit_operator("=");
            self.visit_expr(&value);
        }
    }

    // ── Expression visitor ───────────────────────────────────────────────

    fn visit_expr(&mut self, node: &Node) {
        match node.kind() {
            "binary_expression" => self.visit_binary_op(node),
            "unary_expression" => self.visit_unary_op(node),
            "update_expression" => self.visit_update_expr(node),
            "ternary_expression" => self.visit_ternary(node),
            "assignment_expression" => self.visit_assignment(node),
            "method_invocation" => self.visit_method_invocation(node),
            "object_creation_expression" => self.visit_object_creation(node),
            "field_access" => self.visit_field_access(node),
            "array_access" => self.visit_array_access(node),
            "lambda_expression" => self.visit_lambda(node),
            "method_reference" => self.visit_method_reference(node),
            "identifier" => self.visit_identifier(node),
            "this" => {
                self.events.push(QualitasEvent::Operand(OperandEvent {
                    name: "this".to_string(),
                }));
            }
            "null_literal" => {
                self.events.push(QualitasEvent::Operand(OperandEvent {
                    name: "null".to_string(),
                }));
            }
            "true" | "false" => {
                self.events.push(QualitasEvent::Operand(OperandEvent {
                    name: node_text_owned(node, self.source),
                }));
            }
            "decimal_integer_literal"
            | "hex_integer_literal"
            | "octal_integer_literal"
            | "binary_integer_literal"
            | "decimal_floating_point_literal"
            | "hex_floating_point_literal" => {
                self.visit_literal(node);
            }
            "character_literal" | "string_literal" => {
                self.visit_string_literal(node);
            }
            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    self.visit_expr(&inner);
                }
            }
            "cast_expression" => {
                // Visit the value being cast
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit_expr(&value);
                }
            }
            "instanceof_expression" => self.visit_instanceof(node),
            "array_creation_expression" => self.visit_array_creation(node),
            "conditional_expression" => self.visit_ternary(node),
            _ => self.visit_expr_children(node),
        }
    }

    fn visit_expr_children(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit_expr(&child);
        }
    }

    // ── Operators ────────────────────────────────────────────────────────

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
        self.emit_operator(&op);
        if let Some(operand) = node.child_by_field_name("operand") {
            self.visit_expr(&operand);
        }
    }

    fn visit_update_expr(&mut self, node: &Node) {
        // i++ / i-- / ++i / --i
        let op = self.get_operator_text(node);
        self.emit_operator(&op);
        self.visit_children_exprs(node);
    }

    fn visit_ternary(&mut self, node: &Node) {
        self.events
            .push(QualitasEvent::LogicOp(LogicOpEvent::Ternary));
        self.emit_operator("?:");
        if let Some(cond) = node.child_by_field_name("condition") {
            self.visit_expr(&cond);
        }
        if let Some(consequence) = node.child_by_field_name("consequence") {
            self.visit_expr(&consequence);
        }
        if let Some(alternative) = node.child_by_field_name("alternative") {
            self.visit_expr(&alternative);
        }
    }

    fn visit_assignment(&mut self, node: &Node) {
        let op = self.get_operator_text(node);
        self.emit_operator(&op);
        if let Some(left) = node.child_by_field_name("left") {
            self.visit_expr(&left);
        }
        if let Some(right) = node.child_by_field_name("right") {
            self.visit_expr(&right);
        }
    }

    // ── Calls ────────────────────────────────────────────────────────────

    fn visit_method_invocation(&mut self, node: &Node) {
        // Detect call patterns before visiting children
        if let Some(name_node) = node.child_by_field_name("name") {
            let method_name = node_text(&name_node, self.source);

            // Recursive call check
            if method_name == self.fn_name {
                self.events.push(QualitasEvent::RecursiveCall);
            }

            // API call check: obj.method()
            if let Some(obj) = node.child_by_field_name("object") {
                let obj_name = node_text(&obj, self.source);
                if obj.kind() == "identifier" && self.imported_names.contains(obj_name) {
                    self.events.push(QualitasEvent::ApiCall(ApiCallEvent {
                        object: obj_name.to_string(),
                        method: method_name.to_string(),
                    }));
                }
                self.visit_expr(&obj);
            }
        }

        // Visit arguments
        if let Some(args) = node.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            for arg in args.named_children(&mut cursor) {
                self.visit_expr(&arg);
            }
        }
    }

    fn visit_object_creation(&mut self, node: &Node) {
        self.emit_operator("new");

        // Visit constructor arguments
        if let Some(args) = node.child_by_field_name("arguments") {
            let mut cursor = args.walk();
            for arg in args.named_children(&mut cursor) {
                self.visit_expr(&arg);
            }
        }

        // Anonymous inner class: wrap in NestedFunctionEnter/Exit
        if let Some(class_body) = find_child_by_kind(node, "class_body") {
            self.visit_anonymous_class_body(&class_body);
        }
    }

    fn visit_anonymous_class_body(&mut self, class_body: &Node) {
        self.emit_nested_fn_block(|s| {
            let mut cursor = class_body.walk();
            for child in class_body.named_children(&mut cursor) {
                if child.kind() == "method_declaration" {
                    if let Some(body) = child.child_by_field_name("body") {
                        s.visit_block(&body);
                    }
                }
            }
        });
    }

    fn visit_field_access(&mut self, node: &Node) {
        if let Some(obj) = node.child_by_field_name("object") {
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

    fn visit_array_access(&mut self, node: &Node) {
        self.emit_operator("[]");
        self.visit_children_exprs(node);
    }

    fn visit_lambda(&mut self, node: &Node) {
        if self.nesting_depth > 0 {
            self.events.push(QualitasEvent::NestedCallback);
        }

        self.declare_lambda_params(node);

        self.emit_nested_fn_block(|s| {
            if let Some(body) = node.child_by_field_name("body") {
                if body.kind() == "block" {
                    s.visit_block(&body);
                } else {
                    s.visit_expr(&body);
                }
            }
        });
    }

    fn declare_lambda_params(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "formal_parameters" | "inferred_parameters" => {
                    self.declare_param_list_idents(&child);
                }
                "identifier" => {
                    self.emit_ident_declaration(&child);
                }
                _ => {}
            }
        }
    }

    fn declare_param_list_idents(&mut self, params: &Node) {
        let mut pcursor = params.walk();
        for param in params.named_children(&mut pcursor) {
            if let Some(name_node) = param.child_by_field_name("name") {
                self.emit_ident_declaration(&name_node);
            } else if param.kind() == "identifier" {
                self.emit_ident_declaration(&param);
            }
        }
    }

    fn visit_method_reference(&mut self, node: &Node) {
        self.emit_operator("::");
        // Visit the object side
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "identifier" || child.kind() == "field_access" {
                self.visit_expr(&child);
                break;
            }
        }
    }

    fn visit_instanceof(&mut self, node: &Node) {
        self.emit_operator("instanceof");
        if let Some(left) = node.child_by_field_name("left") {
            self.visit_expr(&left);
        }
    }

    fn visit_array_creation(&mut self, node: &Node) {
        self.emit_operator("new[]");
        self.visit_children_exprs(node);
    }

    // ── Identifiers & literals ───────────────────────────────────────────

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
        let name = text[..text.len().min(32)].to_string();
        self.events
            .push(QualitasEvent::Operand(OperandEvent { name }));
    }

    fn visit_string_literal(&mut self, node: &Node) {
        let text = node_text(node, self.source);
        let name = text[..text.len().min(32)].to_string();
        self.events
            .push(QualitasEvent::Operand(OperandEvent { name }));
    }

    // ── Helpers ──────────────────────────────────────────────────────────

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

    fn emit_ident_declaration(&mut self, node: &Node) {
        let name = node_text(node, self.source);
        if name == "_" {
            return;
        }
        self.events
            .push(QualitasEvent::IdentDeclaration(IdentEvent {
                name: name.to_string(),
                byte_offset: node.start_byte() as u32,
            }));
    }

    fn get_operator_text(&self, node: &Node) -> String {
        const SKIP: &[&str] = &["(", ")", ",", "{", "}", ";", "[", "]"];
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !child.is_named() {
                let text = node_text(&child, self.source);
                if !SKIP.contains(&text) {
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

    fn visit_children_exprs_and_blocks(&mut self, node: &Node) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "block" => self.visit_block(&child),
                "parenthesized_expression" => {
                    if let Some(inner) = child.named_child(0) {
                        self.visit_expr(&inner);
                    }
                }
                _ => self.visit_expr(&child),
            }
        }
    }
}
