/// TypeScript/JavaScript language adapter.
///
/// Uses `oxc_parser` for native-speed AST analysis and emits `QualitasEvents`
/// for the language-agnostic metric collectors.
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, AssignmentExpression, AwaitExpression, BinaryExpression,
    BindingIdentifier, BindingPatternKind, BooleanLiteral, BreakStatement, CallExpression,
    CatchClause, ChainExpression, Class, ConditionalExpression, ContinueStatement,
    DoWhileStatement, ExportDefaultDeclaration, ExportDefaultDeclarationKind, Expression,
    ForInStatement, ForOfStatement, ForStatement, Function, FunctionBody, IdentifierReference,
    IfStatement, ImportDeclaration, ImportDeclarationSpecifier, LabeledStatement,
    LogicalExpression, MethodDefinition, NullLiteral, NumericLiteral, ObjectExpression,
    ObjectPropertyKind, Program, PropertyDefinition, PropertyKey, ReturnStatement, Statement,
    StringLiteral, SwitchStatement, TSAsExpression, TSNonNullExpression, TSTypeAssertion,
    TemplateLiteral, ThisExpression, UnaryExpression, UpdateExpression, VariableDeclarator,
    WhileStatement,
};
use oxc_ast::visit::walk;
use oxc_ast::Visit;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};
use oxc_syntax::scope::ScopeFlags;
use std::collections::HashSet;

use crate::ir::events::{
    ApiCallEvent, AsyncEvent, ControlFlowEvent, ControlFlowKind, IdentEvent, LogicOpEvent,
    OperandEvent, OperatorEvent, QualitasEvent,
};
use crate::ir::language::{
    ClassExtraction, FileExtraction, FunctionExtraction, ImportRecord, LanguageAdapter,
};
use crate::parser::ast::{byte_to_line, count_loc};

pub struct TypeScriptAdapter;

impl LanguageAdapter for TypeScriptAdapter {
    fn name(&self) -> &'static str {
        "TypeScript/JavaScript"
    }

    fn extensions(&self) -> &[&str] {
        &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"]
    }

    fn test_patterns(&self) -> &[&str] {
        &[
            ".test.",
            ".spec.",
            ".playwright-test.",
            "tests/",
            "tests\\",
            "fixtures/",
            "fixtures\\",
        ]
    }

    fn extract(&self, source: &str, file_path: &str) -> Result<FileExtraction, String> {
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(file_path)
            .unwrap_or_else(|_| SourceType::default().with_typescript(true));
        let parse_result = Parser::new(&allocator, source, source_type).parse();

        if !parse_result.errors.is_empty() {
            let msg = parse_result
                .errors
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ");
            eprintln!("qualitas parse warning for {file_path}: {msg}");
        }

        let mut extractor = TsExtractor::new(source);
        extractor.visit_program(&parse_result.program);

        let file_scope = extract_file_scope(
            &parse_result.program,
            source,
            &extractor.imported_names,
        );

        Ok(FileExtraction {
            functions: extractor.functions,
            classes: extractor.classes,
            imports: extractor.imports,
            file_scope,
        })
    }
}

// ─── Top-level extractor ────────────────────────────────────────────────────
// Walks the program to find function/class boundaries and imports.
// For each function body, runs TsBodyEventEmitter to produce events.

struct TsExtractor<'src> {
    source: &'src str,
    functions: Vec<FunctionExtraction>,
    classes: Vec<ClassExtraction>,
    imports: Vec<ImportRecord>,
    class_stack: Vec<usize>,
    /// Imported names (for API call detection inside function bodies)
    imported_names: HashSet<String>,
}

impl<'src> TsExtractor<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            functions: Vec::new(),
            classes: Vec::new(),
            imports: Vec::new(),
            class_stack: Vec::new(),
            imported_names: HashSet::new(),
        }
    }

    fn push_fn(&mut self, fe: FunctionExtraction) {
        if let Some(&ci) = self.class_stack.last() {
            self.classes[ci].methods.push(fe);
        } else {
            self.functions.push(fe);
        }
    }

    /// Process a single object property, extracting arrow/function values by name.
    fn process_object_prop(&mut self, prop: &ObjectPropertyKind<'_>) {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return;
        };
        let name = property_key_name(&p.key);
        match &p.value {
            Expression::ArrowFunctionExpression(arrow) => {
                self.collect_arrow(arrow, name, None);
            }
            Expression::FunctionExpression(f) => {
                let fe = self.extract_function(f, &name, None);
                self.push_fn(fe);
            }
            other => {
                self.visit_expression(other);
            }
        }
    }

    /// Handle init expression for a variable declarator, returning true if handled.
    fn try_extract_var_init(&mut self, init: &Expression<'_>, name: &str) -> bool {
        match init {
            Expression::FunctionExpression(f) => {
                let inferred = Some(format!("const {name} = function"));
                let fe = self.extract_function(f, name, inferred);
                self.push_fn(fe);
                true
            }
            Expression::ArrowFunctionExpression(arrow) => {
                let inferred = Some(format!("const {name} = "));
                self.collect_arrow(arrow, name.to_string(), inferred);
                true
            }
            _ => false,
        }
    }

    /// Analyze a `Function` node (declaration or expression) and produce a
    /// `FunctionExtraction` with its event stream.
    fn extract_function(
        &self,
        func: &Function<'_>,
        name: &str,
        inferred_name: Option<String>,
    ) -> FunctionExtraction {
        let param_count = func.params.items.len() as u32;
        let events = if let Some(body) = &func.body {
            let mut emitter = TsBodyEventEmitter::new(self.source, name, &self.imported_names);
            emitter.visit_function_body(body);
            emitter.events
        } else {
            Vec::new()
        };

        FunctionExtraction {
            name: name.to_string(),
            inferred_name,
            byte_start: func.span.start,
            byte_end: func.span.end,
            start_line: byte_to_line(self.source, func.span.start),
            end_line: byte_to_line(self.source, func.span.end),
            param_count,
            is_async: func.r#async,
            is_generator: func.generator,
            events,
            loc_override: None,
        }
    }

    /// Analyze an `ArrowFunctionExpression` and collect it.
    ///
    /// Calling code must NOT recurse into the arrow body afterwards —
    /// consistent with how `visit_function` returns without walking.
    fn collect_arrow(
        &mut self,
        arrow: &ArrowFunctionExpression<'_>,
        name: String,
        inferred_name: Option<String>,
    ) {
        let param_count = arrow.params.items.len() as u32;
        let body: &FunctionBody = &arrow.body;
        let mut emitter = TsBodyEventEmitter::new(self.source, &name, &self.imported_names);
        emitter.visit_function_body(body);

        self.push_fn(FunctionExtraction {
            name,
            inferred_name,
            byte_start: arrow.span.start,
            byte_end: arrow.span.end,
            start_line: byte_to_line(self.source, arrow.span.start),
            end_line: byte_to_line(self.source, arrow.span.end),
            param_count,
            is_async: arrow.r#async,
            is_generator: false,
            events: emitter.events,
            loc_override: None,
        });
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Extract a displayable name from a property key.
fn property_key_name(key: &PropertyKey<'_>) -> String {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
        PropertyKey::StringLiteral(s) => s.value.to_string(),
        PropertyKey::NumericLiteral(n) => n.value.to_string(),
        _ => "(computed)".to_string(),
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn import_specifier_local_name(spec: &ImportDeclarationSpecifier<'_>) -> String {
    match spec {
        ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => s.local.name.to_string(),
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => s.local.name.to_string(),
        ImportDeclarationSpecifier::ImportSpecifier(s) => s.local.name.to_string(),
    }
}

fn extract_import_names(decl: &ImportDeclaration<'_>) -> Vec<String> {
    decl.specifiers
        .as_ref()
        .map(|specs| specs.iter().map(import_specifier_local_name).collect())
        .unwrap_or_default()
}

// ─── Visitor implementation ──────────────────────────────────────────────────

impl<'a> Visit<'a> for TsExtractor<'_> {
    fn visit_import_declaration(&mut self, decl: &ImportDeclaration<'a>) {
        let source_str = decl.source.value.as_str();
        let is_ext = !source_str.starts_with('.') && !source_str.starts_with('/');

        let names = extract_import_names(decl);
        for n in &names {
            self.imported_names.insert(n.clone());
        }

        self.imports.push(ImportRecord {
            source: source_str.to_string(),
            is_external: is_ext,
            names,
        });
    }

    fn visit_function(&mut self, func: &Function<'a>, _flags: ScopeFlags) {
        let name = func
            .id
            .as_ref()
            .map_or("(anonymous)", |id| id.name.as_str())
            .to_string();
        let fe = self.extract_function(func, &name, None);
        self.push_fn(fe);
        // Don't recurse — nested functions are collected separately
    }

    /// `const foo = () => {}` and `const foo = function() {}`
    ///
    /// Also recurses for other initialisers (object literals, etc.) so that
    /// nested patterns handled by other overrides are still visited.
    fn visit_variable_declarator(&mut self, decl: &VariableDeclarator<'a>) {
        let name = if let BindingPatternKind::BindingIdentifier(id) = &decl.id.kind {
            id.name.to_string()
        } else {
            walk::walk_variable_declarator(self, decl);
            return;
        };

        if let Some(init) = &decl.init {
            if self.try_extract_var_init(init, &name) {
                return;
            }
        }

        walk::walk_variable_declarator(self, decl);
    }

    /// `{ method: () => {} }` and `{ helper: function() {} }`
    ///
    /// Iterates properties explicitly so each arrow/function value gets the
    /// property key as its name, and we avoid recursing into collected bodies.
    fn visit_object_expression(&mut self, obj: &ObjectExpression<'a>) {
        for prop in &obj.properties {
            self.process_object_prop(prop);
        }
    }

    /// `export default () => {}` and `export default function() {}`
    fn visit_export_default_declaration(&mut self, decl: &ExportDefaultDeclaration<'a>) {
        match &decl.declaration {
            ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => {
                self.collect_arrow(
                    arrow,
                    "(default)".to_string(),
                    Some("export default ".to_string()),
                );
            }
            ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                let name =
                    f.id.as_ref()
                        .map_or("(default)", |id| id.name.as_str())
                        .to_string();
                let fe = self.extract_function(f, &name, Some("export default ".to_string()));
                self.push_fn(fe);
            }
            _ => {
                walk::walk_export_default_declaration(self, decl);
            }
        }
    }

    /// `class Foo { method(args) {} }` — class method definitions.
    ///
    /// The `Function` node inside a `MethodDefinition` has `id = None`, so the
    /// name must come from `MethodDefinition.key`. By overriding here (without
    /// recursing) we avoid `visit_function` giving the method an "(anonymous)" name.
    fn visit_method_definition(&mut self, method: &MethodDefinition<'a>) {
        let name = property_key_name(&method.key);
        let fe = self.extract_function(&method.value, &name, None);
        self.push_fn(fe);
    }

    /// `class Foo { method = () => {} }` — class property arrows.
    ///
    /// Regular class methods (`foo() {}`) are handled by `visit_method_definition`.
    fn visit_property_definition(&mut self, prop: &PropertyDefinition<'a>) {
        if let Some(Expression::ArrowFunctionExpression(arrow)) = &prop.value {
            let name = property_key_name(&prop.key);
            self.collect_arrow(arrow, name, None);
        } else {
            walk::walk_property_definition(self, prop);
        }
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        let name = class
            .id
            .as_ref()
            .map_or("(anonymous class)", |id| id.name.as_str())
            .to_string();

        let ce = ClassExtraction {
            name,
            byte_start: class.span.start,
            byte_end: class.span.end,
            start_line: byte_to_line(self.source, class.span.start),
            end_line: byte_to_line(self.source, class.span.end),
            methods: Vec::new(),
        };
        let idx = self.classes.len();
        self.classes.push(ce);
        self.class_stack.push(idx);

        walk::walk_class(self, class);

        self.class_stack.pop();
    }
}

// ─── File-scope extraction ──────────────────────────────────────────────────
// Second pass over the program body to capture top-level executable code.

/// Returns true for top-level executable statements (not declarations).
fn is_executable_statement(stmt: &Statement<'_>) -> bool {
    matches!(
        stmt,
        Statement::ExpressionStatement(_)
            | Statement::IfStatement(_)
            | Statement::SwitchStatement(_)
            | Statement::ForStatement(_)
            | Statement::ForInStatement(_)
            | Statement::ForOfStatement(_)
            | Statement::WhileStatement(_)
            | Statement::DoWhileStatement(_)
            | Statement::TryStatement(_)
            | Statement::ThrowStatement(_)
            | Statement::BlockStatement(_)
            | Statement::LabeledStatement(_)
            | Statement::ReturnStatement(_)
    )
}

/// Extract file-scope analysis for top-level executable code.
///
/// Iterates over `program.body`, skipping declarations, and collects events
/// from executable statements (control flow, expression statements, try/catch).
/// Returns `None` if no executable statements produce events.
fn extract_file_scope(
    program: &Program<'_>,
    source: &str,
    imported_names: &HashSet<String>,
) -> Option<FunctionExtraction> {
    let mut emitter = TsBodyEventEmitter::new(source, "<file-scope>", imported_names);
    let mut loc_sum: u32 = 0;
    let mut has_statements = false;
    let mut min_start: u32 = u32::MAX;
    let mut max_end: u32 = 0;

    for stmt in &program.body {
        if !is_executable_statement(stmt) {
            continue;
        }
        has_statements = true;

        let span = stmt.span();
        loc_sum += count_loc(source, span.start, span.end);
        min_start = min_start.min(span.start);
        max_end = max_end.max(span.end);

        emitter.visit_statement(stmt);
    }

    if !has_statements || emitter.events.is_empty() {
        return None;
    }

    Some(FunctionExtraction {
        name: "<file-scope>".to_string(),
        inferred_name: None,
        byte_start: min_start,
        byte_end: max_end,
        start_line: byte_to_line(source, min_start),
        end_line: byte_to_line(source, max_end),
        param_count: 0,
        is_async: false,
        is_generator: false,
        events: emitter.events,
        loc_override: Some(loc_sum),
    })
}

// ─── Function body event emitter ────────────────────────────────────────────
// Walks a single function body and emits all QualitasEvents.
// This replaces the 4 separate metric visitors (CFC, DCI, IRC, SM)
// with a single AST pass that produces an event stream.

struct TsBodyEventEmitter<'src> {
    events: Vec<QualitasEvent>,
    fn_name: String,
    #[allow(dead_code)]
    source: &'src str,
    imported_names: &'src HashSet<String>,
    /// CFC nesting depth (for `NestedCallback` detection)
    nesting_depth: u32,
}

impl<'src> TsBodyEventEmitter<'src> {
    fn new(source: &'src str, fn_name: &str, imported_names: &'src HashSet<String>) -> Self {
        Self {
            events: Vec::with_capacity(256),
            fn_name: fn_name.to_string(),
            source,
            imported_names,
            nesting_depth: 0,
        }
    }

    fn detect_recursive_call(&mut self, callee: &Expression<'_>) {
        if let Expression::Identifier(id) = callee {
            if !self.fn_name.is_empty() && id.name.as_str() == self.fn_name {
                self.events.push(QualitasEvent::RecursiveCall);
            }
        }
    }

    fn detect_member_call_patterns(&mut self, callee: &Expression<'_>) {
        let Expression::StaticMemberExpression(member) = callee else {
            return;
        };
        let prop = member.property.name.as_str();
        if prop == "then" || prop == "catch" {
            self.events
                .push(QualitasEvent::AsyncComplexity(AsyncEvent::PromiseChain));
        }
        if let Expression::Identifier(obj) = &member.object {
            let obj_name = obj.name.as_str();
            if self.imported_names.contains(obj_name) {
                self.events.push(QualitasEvent::ApiCall(ApiCallEvent {
                    object: obj_name.to_string(),
                    method: prop.to_string(),
                }));
            }
        }
    }
}

impl<'a> Visit<'a> for TsBodyEventEmitter<'_> {
    // ── CFC: Control flow ───────────────────────────────────────────────

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::If,
                has_else: it.alternate.is_some(),
                else_is_if: matches!(&it.alternate, Some(Statement::IfStatement(_))),
            }));

        // Visit test expression — captures &&/||, operands, operators
        self.visit_expression(&it.test);

        // Nesting for the consequent body
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        self.visit_statement(&it.consequent);

        if let Some(alt) = &it.alternate {
            match alt {
                Statement::IfStatement(_) => {
                    // else-if: +1 flat, then recursively visit the inner if
                    self.events
                        .push(QualitasEvent::LogicOp(LogicOpEvent::Ternary));
                    // NOTE: The inner IfStatement visit will emit its own ControlFlow(If)
                    // via visit_if_statement, adding the nesting-aware increment.
                    // The LogicOp above is +1 flat for the else-if branch.
                    // This matches the original CFC behavior: add_flat() + recursive visit_if_statement
                    self.visit_statement(alt);
                }
                other => {
                    self.visit_statement(other);
                }
            }
        }

        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::For,
                has_else: false,
                else_is_if: false,
            }));
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        walk::walk_for_statement(self, it);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ForIn,
                has_else: false,
                else_is_if: false,
            }));
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        walk::walk_for_in_statement(self, it);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ForOf,
                has_else: false,
                else_is_if: false,
            }));
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        walk::walk_for_of_statement(self, it);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_while_statement(&mut self, it: &WhileStatement<'a>) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::While,
                has_else: false,
                else_is_if: false,
            }));
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        walk::walk_while_statement(self, it);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_do_while_statement(&mut self, it: &DoWhileStatement<'a>) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::DoWhile,
                has_else: false,
                else_is_if: false,
            }));
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        walk::walk_do_while_statement(self, it);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_switch_statement(&mut self, it: &SwitchStatement<'a>) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::Switch,
                has_else: false,
                else_is_if: false,
            }));
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        walk::walk_switch_statement(self, it);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::Catch,
                has_else: false,
                else_is_if: false,
            }));
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        walk::walk_catch_clause(self, it);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    // ── CFC: Logic operators ────────────────────────────────────────────

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        let op = match it.operator.as_str() {
            "&&" => LogicOpEvent::And,
            "||" => LogicOpEvent::Or,
            "??" => LogicOpEvent::NullCoalesce,
            _ => LogicOpEvent::And, // fallback
        };
        self.events.push(QualitasEvent::LogicOp(op));
        // DCI: also record as an operator
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: it.operator.as_str().to_string(),
        }));
        walk::walk_logical_expression(self, it);
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        // CFC: +1 flat for ternary
        self.events
            .push(QualitasEvent::LogicOp(LogicOpEvent::Ternary));
        // DCI: record as operator
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: "?:".to_string(),
        }));
        walk::walk_conditional_expression(self, it);
    }

    fn visit_labeled_statement(&mut self, it: &LabeledStatement<'a>) {
        self.events.push(QualitasEvent::LabeledFlow);
        walk::walk_labeled_statement(self, it);
    }

    fn visit_break_statement(&mut self, it: &BreakStatement) {
        if it.label.is_some() {
            self.events.push(QualitasEvent::LabeledFlow);
        }
    }

    fn visit_continue_statement(&mut self, it: &ContinueStatement) {
        if it.label.is_some() {
            self.events.push(QualitasEvent::LabeledFlow);
        }
    }

    // ── CFC: Calls (recursive, promise chains, API calls) ───────────────

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        self.detect_recursive_call(&it.callee);
        self.detect_member_call_patterns(&it.callee);
        walk::walk_call_expression(self, it);
    }

    // ── CFC: Arrow functions (nested callback penalty) ──────────────────

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        // Nested callback: add nesting_depth penalty
        if self.nesting_depth > 0 {
            self.events.push(QualitasEvent::NestedCallback);
        }

        // Mark nested function boundary (SM and IRC stop here)
        self.events.push(QualitasEvent::NestedFunctionEnter);
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        walk::walk_arrow_function_expression(self, it);
        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
        self.events.push(QualitasEvent::NestedFunctionExit);
    }

    // ── CFC: Await expression ───────────────────────────────────────────

    fn visit_await_expression(&mut self, it: &AwaitExpression<'a>) {
        if self.nesting_depth > 1 {
            self.events
                .push(QualitasEvent::AsyncComplexity(AsyncEvent::Await));
        }
        walk::walk_await_expression(self, it);
    }

    // ── DCI: Operators ──────────────────────────────────────────────────

    fn visit_binary_expression(&mut self, it: &BinaryExpression<'a>) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: it.operator.as_str().to_string(),
        }));
        walk::walk_binary_expression(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: it.operator.as_str().to_string(),
        }));
        walk::walk_assignment_expression(self, it);
    }

    fn visit_unary_expression(&mut self, it: &UnaryExpression<'a>) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: it.operator.as_str().to_string(),
        }));
        walk::walk_unary_expression(self, it);
    }

    fn visit_update_expression(&mut self, it: &UpdateExpression<'a>) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: if it.prefix {
                "++pre".to_string()
            } else {
                "post++".to_string()
            },
        }));
        walk::walk_update_expression(self, it);
    }

    fn visit_chain_expression(&mut self, it: &ChainExpression<'a>) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: "?.".to_string(),
        }));
        walk::walk_chain_expression(self, it);
    }

    fn visit_ts_as_expression(&mut self, it: &TSAsExpression<'a>) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: "as".to_string(),
        }));
        walk::walk_ts_as_expression(self, it);
    }

    fn visit_ts_type_assertion(&mut self, it: &TSTypeAssertion<'a>) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: "<type>".to_string(),
        }));
        walk::walk_ts_type_assertion(self, it);
    }

    fn visit_ts_non_null_expression(&mut self, it: &TSNonNullExpression<'a>) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: "!".to_string(),
        }));
        walk::walk_ts_non_null_expression(self, it);
    }

    // ── DCI: Operands ───────────────────────────────────────────────────

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        let name = it.name.as_str();
        // DCI: operand
        self.events.push(QualitasEvent::Operand(OperandEvent {
            name: name.to_string(),
        }));
        // IRC: reference
        self.events.push(QualitasEvent::IdentReference(IdentEvent {
            name: name.to_string(),
            byte_offset: it.span.start,
        }));
    }

    fn visit_string_literal(&mut self, it: &StringLiteral<'a>) {
        let key = &it.value.as_str()[..it.value.len().min(32)];
        self.events.push(QualitasEvent::Operand(OperandEvent {
            name: key.to_string(),
        }));
    }

    fn visit_numeric_literal(&mut self, it: &NumericLiteral<'a>) {
        self.events.push(QualitasEvent::Operand(OperandEvent {
            name: it.value.to_string(),
        }));
    }

    fn visit_boolean_literal(&mut self, it: &BooleanLiteral) {
        self.events.push(QualitasEvent::Operand(OperandEvent {
            name: if it.value { "true" } else { "false" }.to_string(),
        }));
    }

    fn visit_null_literal(&mut self, _it: &NullLiteral) {
        self.events.push(QualitasEvent::Operand(OperandEvent {
            name: "null".to_string(),
        }));
    }

    fn visit_this_expression(&mut self, _it: &ThisExpression) {
        self.events.push(QualitasEvent::Operand(OperandEvent {
            name: "this".to_string(),
        }));
    }

    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        self.events.push(QualitasEvent::Operand(OperandEvent {
            name: format!("tmpl#{}", it.span.start),
        }));
        walk::walk_template_literal(self, it);
    }

    // ── IRC: Declarations ───────────────────────────────────────────────

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.events
            .push(QualitasEvent::IdentDeclaration(IdentEvent {
                name: it.name.as_str().to_string(),
                byte_offset: it.span.start,
            }));
    }

    // ── SM: Return statements ───────────────────────────────────────────

    fn visit_return_statement(&mut self, it: &ReturnStatement<'a>) {
        self.events.push(QualitasEvent::ReturnStatement);
        walk::walk_return_statement(self, it);
    }
}
