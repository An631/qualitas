/// Rust language adapter.
///
/// Uses `syn` for native-speed AST analysis and emits `QualitasEvent`s
/// for the language-agnostic metric collectors.
use std::collections::HashSet;

use syn::visit::Visit;
use syn::{
    Arm, BinOp, Block, Expr, ExprBinary, ExprCall, ExprIf, ExprMethodCall, File, FnArg, ImplItem,
    ImplItemFn, Item, ItemFn, ItemImpl, ItemUse, Lit, Pat, Stmt, UnOp, UseTree,
};

use crate::ir::events::{
    ApiCallEvent, AsyncEvent, ControlFlowEvent, ControlFlowKind, IdentEvent, LogicOpEvent,
    OperandEvent, OperatorEvent, QualitasEvent,
};
use crate::ir::language::{
    ClassExtraction, FileExtraction, FunctionExtraction, ImportRecord, LanguageAdapter,
};

pub struct RustAdapter;

impl LanguageAdapter for RustAdapter {
    fn name(&self) -> &'static str {
        "Rust"
    }

    fn extensions(&self) -> &[&str] {
        &[".rs"]
    }

    fn extract(&self, source: &str, file_path: &str) -> Result<FileExtraction, String> {
        let syntax: File = syn::parse_str(source)
            .map_err(|e| format!("qualitas parse error for {file_path}: {e}"))?;

        let mut extractor = RustExtractor::new(source);
        extractor.visit_file(&syntax);

        Ok(FileExtraction {
            functions: extractor.functions,
            classes: extractor.classes,
            imports: extractor.imports,
        })
    }
}

// ─── Helper: span → line number ──────────────────────────────────────────────

fn span_to_line(span: proc_macro2::Span) -> u32 {
    span.start().line as u32
}

fn span_to_end_line(span: proc_macro2::Span) -> u32 {
    span.end().line as u32
}

/// Approximate byte offset from a proc_macro2::Span start position.
/// We walk the source to find the byte offset at (line, column).
fn line_col_to_byte(source: &str, line: usize, col: usize) -> u32 {
    let mut current_line = 1;
    let mut byte_offset = 0;
    for (i, ch) in source.char_indices() {
        if current_line == line {
            // Column is 0-based in proc_macro2
            let col_offset = source[i..].char_indices().nth(col).map_or(0, |(o, _)| o);
            return (i + col_offset) as u32;
        }
        if ch == '\n' {
            current_line += 1;
        }
        byte_offset = i + ch.len_utf8();
    }
    byte_offset as u32
}

fn span_to_byte_start(source: &str, span: proc_macro2::Span) -> u32 {
    line_col_to_byte(source, span.start().line, span.start().column)
}

fn span_to_byte_end(source: &str, span: proc_macro2::Span) -> u32 {
    line_col_to_byte(source, span.end().line, span.end().column)
}

// ─── Use-tree name extraction ────────────────────────────────────────────────

fn collect_use_names(tree: &UseTree, prefix: &str, names: &mut Vec<String>) {
    match tree {
        UseTree::Path(p) => {
            let new_prefix = if prefix.is_empty() {
                p.ident.to_string()
            } else {
                format!("{prefix}::{}", p.ident)
            };
            collect_use_names(&p.tree, &new_prefix, names);
        }
        UseTree::Name(n) => {
            names.push(n.ident.to_string());
        }
        UseTree::Rename(r) => {
            names.push(r.rename.to_string());
        }
        UseTree::Glob(_) => {
            names.push(format!("{prefix}::*"));
        }
        UseTree::Group(g) => {
            for item in &g.items {
                collect_use_names(item, prefix, names);
            }
        }
    }
}

/// Extract the top-level crate/module name from a use tree.
fn use_tree_source(tree: &UseTree) -> String {
    match tree {
        UseTree::Path(p) => p.ident.to_string(),
        UseTree::Name(n) => n.ident.to_string(),
        UseTree::Rename(r) => r.ident.to_string(),
        UseTree::Glob(_) => "*".to_string(),
        UseTree::Group(_) => "(group)".to_string(),
    }
}

// ─── Top-level extractor ────────────────────────────────────────────────────

struct RustExtractor<'src> {
    source: &'src str,
    functions: Vec<FunctionExtraction>,
    classes: Vec<ClassExtraction>,
    imports: Vec<ImportRecord>,
    /// Names imported via `use` statements (for API call detection)
    imported_names: HashSet<String>,
    /// Current impl block index (like class_stack in TS adapter)
    impl_stack: Vec<usize>,
}

impl<'src> RustExtractor<'src> {
    fn new(source: &'src str) -> Self {
        Self {
            source,
            functions: Vec::new(),
            classes: Vec::new(),
            imports: Vec::new(),
            imported_names: HashSet::new(),
            impl_stack: Vec::new(),
        }
    }

    fn push_fn(&mut self, fe: FunctionExtraction) {
        if let Some(&ci) = self.impl_stack.last() {
            self.classes[ci].methods.push(fe);
        } else {
            self.functions.push(fe);
        }
    }

    fn extract_fn(
        &self,
        name: &str,
        sig: &syn::Signature,
        block: &Block,
        span: proc_macro2::Span,
    ) -> FunctionExtraction {
        let param_count = sig.inputs.len() as u32;
        let is_async = sig.asyncness.is_some();

        let mut emitter = RustBodyEventEmitter::new(self.source, name, &self.imported_names);
        emitter.visit_block(block);

        FunctionExtraction {
            name: name.to_string(),
            inferred_name: None,
            byte_start: span_to_byte_start(self.source, span),
            byte_end: span_to_byte_end(self.source, span),
            start_line: span_to_line(span),
            end_line: span_to_end_line(span),
            param_count,
            is_async,
            is_generator: false, // Rust doesn't have generators (stable)
            events: emitter.events,
        }
    }
}

impl<'ast> Visit<'ast> for RustExtractor<'_> {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let source_name = use_tree_source(&node.tree);
        let is_external = source_name != "crate" && source_name != "self" && source_name != "super";

        let mut names = Vec::new();
        collect_use_names(&node.tree, "", &mut names);

        for n in &names {
            self.imported_names.insert(n.clone());
        }

        self.imports.push(ImportRecord {
            source: source_name,
            is_external,
            names,
        });
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let name = node.sig.ident.to_string();
        let fe = self.extract_fn(&name, &node.sig, &node.block, node.sig.ident.span());
        // Use the full item span for byte offsets
        let fe = FunctionExtraction {
            byte_start: span_to_byte_start(self.source, node.sig.fn_token.span),
            byte_end: span_to_byte_end(self.source, node.block.brace_token.span.close()),
            start_line: span_to_line(node.sig.fn_token.span),
            end_line: span_to_end_line(node.block.brace_token.span.close()),
            ..fe
        };
        self.push_fn(fe);
        // Don't recurse — nested functions in bodies are handled by the event emitter
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        // Extract the type name for this impl block
        let name = if let syn::Type::Path(tp) = node.self_ty.as_ref() {
            tp.path
                .segments
                .last()
                .map_or("(anonymous impl)".to_string(), |s| s.ident.to_string())
        } else {
            "(anonymous impl)".to_string()
        };

        let span = node.impl_token.span;
        let end_span = node.brace_token.span.close();

        let ce = ClassExtraction {
            name,
            byte_start: span_to_byte_start(self.source, span),
            byte_end: span_to_byte_end(self.source, end_span),
            start_line: span_to_line(span),
            end_line: span_to_end_line(end_span),
            methods: Vec::new(),
        };
        let idx = self.classes.len();
        self.classes.push(ce);
        self.impl_stack.push(idx);

        // Visit each impl item manually to handle methods
        for item in &node.items {
            if let ImplItem::Fn(method) = item {
                self.visit_impl_item_fn(method);
            }
        }

        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        let name = node.sig.ident.to_string();

        // Count params excluding `self`
        let param_count = node
            .sig
            .inputs
            .iter()
            .filter(|arg| matches!(arg, FnArg::Typed(_)))
            .count() as u32;

        let is_async = node.sig.asyncness.is_some();

        let mut emitter = RustBodyEventEmitter::new(self.source, &name, &self.imported_names);
        emitter.visit_block(&node.block);

        let span = node.sig.fn_token.span;
        let end_span = node.block.brace_token.span.close();

        let fe = FunctionExtraction {
            name,
            inferred_name: None,
            byte_start: span_to_byte_start(self.source, span),
            byte_end: span_to_byte_end(self.source, end_span),
            start_line: span_to_line(span),
            end_line: span_to_end_line(end_span),
            param_count,
            is_async,
            is_generator: false,
            events: emitter.events,
        };
        self.push_fn(fe);
    }
}

// ─── Function body event emitter ────────────────────────────────────────────
// Walks a single function body and emits all QualitasEvents.

struct RustBodyEventEmitter<'src> {
    events: Vec<QualitasEvent>,
    fn_name: String,
    source: &'src str,
    imported_names: &'src HashSet<String>,
    nesting_depth: u32,
}

impl<'src> RustBodyEventEmitter<'src> {
    fn new(source: &'src str, fn_name: &str, imported_names: &'src HashSet<String>) -> Self {
        Self {
            events: Vec::with_capacity(256),
            fn_name: fn_name.to_string(),
            source,
            imported_names,
            nesting_depth: 0,
        }
    }

    /// Emit ident declaration events for all bindings in a pattern.
    fn emit_pattern_bindings(&mut self, pat: &Pat) {
        match pat {
            Pat::Ident(pi) => {
                self.events
                    .push(QualitasEvent::IdentDeclaration(IdentEvent {
                        name: pi.ident.to_string(),
                        byte_offset: span_to_byte_start(self.source, pi.ident.span()),
                    }));
            }
            Pat::Tuple(pt) => {
                for elem in &pt.elems {
                    self.emit_pattern_bindings(elem);
                }
            }
            Pat::TupleStruct(pts) => {
                for elem in &pts.elems {
                    self.emit_pattern_bindings(elem);
                }
            }
            Pat::Struct(ps) => {
                for field in &ps.fields {
                    self.emit_pattern_bindings(&field.pat);
                }
            }
            Pat::Slice(ps) => {
                for elem in &ps.elems {
                    self.emit_pattern_bindings(elem);
                }
            }
            Pat::Or(po) => {
                for case in &po.cases {
                    self.emit_pattern_bindings(case);
                }
            }
            Pat::Reference(pr) => {
                self.emit_pattern_bindings(&pr.pat);
            }
            Pat::Type(pt) => {
                self.emit_pattern_bindings(&pt.pat);
            }
            _ => {}
        }
    }

    /// Visit an expression, emitting events for all sub-expressions.
    fn visit_expr_inner(&mut self, expr: &Expr) {
        match expr {
            // ── Control flow ──────────────────────────────────────────────
            Expr::If(expr_if) => self.visit_if(expr_if),

            Expr::ForLoop(expr_for) => {
                self.events
                    .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                        kind: ControlFlowKind::ForOf,
                        has_else: false,
                        else_is_if: false,
                    }));
                // Emit binding for the loop variable
                self.emit_pattern_bindings(&expr_for.pat);
                // Visit the iterator expression
                self.visit_expr_inner(&expr_for.expr);
                // Nesting for the loop body
                self.events.push(QualitasEvent::NestingEnter);
                self.nesting_depth += 1;
                self.visit_block_stmts(&expr_for.body);
                self.nesting_depth -= 1;
                self.events.push(QualitasEvent::NestingExit);
            }

            Expr::While(expr_while) => {
                self.events
                    .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                        kind: ControlFlowKind::While,
                        has_else: false,
                        else_is_if: false,
                    }));
                self.visit_expr_inner(&expr_while.cond);
                self.events.push(QualitasEvent::NestingEnter);
                self.nesting_depth += 1;
                self.visit_block_stmts(&expr_while.body);
                self.nesting_depth -= 1;
                self.events.push(QualitasEvent::NestingExit);
            }

            Expr::Loop(expr_loop) => {
                // `loop {}` is like `while(true) {}`
                self.events
                    .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                        kind: ControlFlowKind::While,
                        has_else: false,
                        else_is_if: false,
                    }));
                self.events.push(QualitasEvent::NestingEnter);
                self.nesting_depth += 1;
                self.visit_block_stmts(&expr_loop.body);
                self.nesting_depth -= 1;
                self.events.push(QualitasEvent::NestingExit);
            }

            Expr::Match(expr_match) => {
                self.events
                    .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                        kind: ControlFlowKind::PatternMatch,
                        has_else: false,
                        else_is_if: false,
                    }));
                self.visit_expr_inner(&expr_match.expr);
                self.events.push(QualitasEvent::NestingEnter);
                self.nesting_depth += 1;
                for arm in &expr_match.arms {
                    self.visit_match_arm(arm);
                }
                self.nesting_depth -= 1;
                self.events.push(QualitasEvent::NestingExit);
            }

            // ── Logic operators ───────────────────────────────────────────
            Expr::Binary(expr_bin) => {
                self.visit_binary(expr_bin);
            }

            // ── Unary operators ───────────────────────────────────────────
            Expr::Unary(expr_un) => {
                let op_name = match expr_un.op {
                    UnOp::Not(_) => "!",
                    UnOp::Neg(_) => "-",
                    UnOp::Deref(_) => "*",
                    _ => "?unary",
                };
                self.events.push(QualitasEvent::Operator(OperatorEvent {
                    name: op_name.to_string(),
                }));
                self.visit_expr_inner(&expr_un.expr);
            }

            // ── Reference (&, &mut) ──────────────────────────────────────
            Expr::Reference(expr_ref) => {
                let op_name = if expr_ref.mutability.is_some() {
                    "&mut"
                } else {
                    "&"
                };
                self.events.push(QualitasEvent::Operator(OperatorEvent {
                    name: op_name.to_string(),
                }));
                self.visit_expr_inner(&expr_ref.expr);
            }

            // ── Assignment ───────────────────────────────────────────────
            Expr::Assign(expr_assign) => {
                self.events.push(QualitasEvent::Operator(OperatorEvent {
                    name: "=".to_string(),
                }));
                self.visit_expr_inner(&expr_assign.left);
                self.visit_expr_inner(&expr_assign.right);
            }

            // ── Range (.. / ..=) ─────────────────────────────────────────
            Expr::Range(expr_range) => {
                self.events.push(QualitasEvent::Operator(OperatorEvent {
                    name: "..".to_string(),
                }));
                if let Some(start) = &expr_range.start {
                    self.visit_expr_inner(start);
                }
                if let Some(end) = &expr_range.end {
                    self.visit_expr_inner(end);
                }
            }

            // ── Cast (as) ────────────────────────────────────────────────
            Expr::Cast(expr_cast) => {
                self.events.push(QualitasEvent::Operator(OperatorEvent {
                    name: "as".to_string(),
                }));
                self.visit_expr_inner(&expr_cast.expr);
            }

            // ── Try operator (?) ─────────────────────────────────────────
            Expr::Try(expr_try) => {
                // The ? operator is an early-return control flow
                self.events.push(QualitasEvent::ReturnStatement);
                self.events.push(QualitasEvent::Operator(OperatorEvent {
                    name: "?".to_string(),
                }));
                self.visit_expr_inner(&expr_try.expr);
            }

            // ── Return/Break/Continue ────────────────────────────────────
            Expr::Return(expr_ret) => {
                self.events.push(QualitasEvent::ReturnStatement);
                if let Some(val) = &expr_ret.expr {
                    self.visit_expr_inner(val);
                }
            }

            Expr::Break(expr_break) => {
                if expr_break.label.is_some() {
                    self.events.push(QualitasEvent::LabeledFlow);
                }
                if let Some(val) = &expr_break.expr {
                    self.visit_expr_inner(val);
                }
            }

            Expr::Continue(expr_cont) => {
                if expr_cont.label.is_some() {
                    self.events.push(QualitasEvent::LabeledFlow);
                }
            }

            // ── Calls ────────────────────────────────────────────────────
            Expr::Call(expr_call) => {
                self.visit_call(expr_call);
            }

            Expr::MethodCall(expr_method) => {
                self.visit_method_call(expr_method);
            }

            // ── Closures (nested functions) ──────────────────────────────
            Expr::Closure(expr_closure) => {
                if self.nesting_depth > 0 {
                    self.events.push(QualitasEvent::NestedCallback);
                }
                self.events.push(QualitasEvent::NestedFunctionEnter);
                self.events.push(QualitasEvent::NestingEnter);
                self.nesting_depth += 1;
                self.visit_expr_inner(&expr_closure.body);
                self.nesting_depth -= 1;
                self.events.push(QualitasEvent::NestingExit);
                self.events.push(QualitasEvent::NestedFunctionExit);
            }

            // ── Async blocks ─────────────────────────────────────────────
            Expr::Async(expr_async) => {
                self.events
                    .push(QualitasEvent::AsyncComplexity(AsyncEvent::Spawn));
                self.events.push(QualitasEvent::NestedFunctionEnter);
                self.events.push(QualitasEvent::NestingEnter);
                self.nesting_depth += 1;
                self.visit_block_stmts(&expr_async.block);
                self.nesting_depth -= 1;
                self.events.push(QualitasEvent::NestingExit);
                self.events.push(QualitasEvent::NestedFunctionExit);
            }

            // ── Await ────────────────────────────────────────────────────
            Expr::Await(expr_await) => {
                if self.nesting_depth > 1 {
                    self.events
                        .push(QualitasEvent::AsyncComplexity(AsyncEvent::Await));
                }
                self.visit_expr_inner(&expr_await.base);
            }

            // ── Identifiers / paths (operands + IRC) ─────────────────────
            Expr::Path(expr_path) => {
                if let Some(ident) = expr_path.path.get_ident() {
                    let name = ident.to_string();
                    // DCI: operand
                    self.events
                        .push(QualitasEvent::Operand(OperandEvent { name: name.clone() }));
                    // IRC: reference
                    self.events.push(QualitasEvent::IdentReference(IdentEvent {
                        name,
                        byte_offset: span_to_byte_start(self.source, ident.span()),
                    }));
                } else if expr_path.path.segments.len() > 1 {
                    // Multi-segment path like `module::function` — record as operand
                    let full = expr_path
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    self.events
                        .push(QualitasEvent::Operand(OperandEvent { name: full }));
                }
            }

            // ── Literals (operands) ──────────────────────────────────────
            Expr::Lit(expr_lit) => match &expr_lit.lit {
                Lit::Int(i) => {
                    self.events.push(QualitasEvent::Operand(OperandEvent {
                        name: i.to_string(),
                    }));
                }
                Lit::Float(f) => {
                    self.events.push(QualitasEvent::Operand(OperandEvent {
                        name: f.to_string(),
                    }));
                }
                Lit::Str(s) => {
                    let val = s.value();
                    let key = &val[..val.len().min(32)];
                    self.events.push(QualitasEvent::Operand(OperandEvent {
                        name: key.to_string(),
                    }));
                }
                Lit::Bool(b) => {
                    self.events.push(QualitasEvent::Operand(OperandEvent {
                        name: if b.value { "true" } else { "false" }.to_string(),
                    }));
                }
                Lit::Char(c) => {
                    self.events.push(QualitasEvent::Operand(OperandEvent {
                        name: c.value().to_string(),
                    }));
                }
                Lit::Byte(b) => {
                    self.events.push(QualitasEvent::Operand(OperandEvent {
                        name: b.value().to_string(),
                    }));
                }
                _ => {}
            },

            // ── Block expressions ────────────────────────────────────────
            Expr::Block(expr_block) => {
                self.visit_block_stmts(&expr_block.block);
            }

            Expr::Unsafe(expr_unsafe) => {
                self.visit_block_stmts(&expr_unsafe.block);
            }

            // ── Field access ─────────────────────────────────────────────
            Expr::Field(expr_field) => {
                self.visit_expr_inner(&expr_field.base);
            }

            // ── Index ────────────────────────────────────────────────────
            Expr::Index(expr_index) => {
                self.events.push(QualitasEvent::Operator(OperatorEvent {
                    name: "[]".to_string(),
                }));
                self.visit_expr_inner(&expr_index.expr);
                self.visit_expr_inner(&expr_index.index);
            }

            // ── Tuple / Array / Struct construction ──────────────────────
            Expr::Tuple(expr_tuple) => {
                for elem in &expr_tuple.elems {
                    self.visit_expr_inner(elem);
                }
            }

            Expr::Array(expr_array) => {
                for elem in &expr_array.elems {
                    self.visit_expr_inner(elem);
                }
            }

            Expr::Struct(expr_struct) => {
                for field in &expr_struct.fields {
                    self.visit_expr_inner(&field.expr);
                }
                if let Some(rest) = &expr_struct.rest {
                    self.visit_expr_inner(rest);
                }
            }

            // ── Paren / Group ────────────────────────────────────────────
            Expr::Paren(expr_paren) => {
                self.visit_expr_inner(&expr_paren.expr);
            }

            Expr::Group(expr_group) => {
                self.visit_expr_inner(&expr_group.expr);
            }

            // ── Let expressions (used in if-let, while-let) ──────────────
            Expr::Let(expr_let) => {
                self.emit_pattern_bindings(&expr_let.pat);
                self.visit_expr_inner(&expr_let.expr);
            }

            // ── Repeat [expr; count] ─────────────────────────────────────
            Expr::Repeat(expr_repeat) => {
                self.visit_expr_inner(&expr_repeat.expr);
                self.visit_expr_inner(&expr_repeat.len);
            }

            // ── Macro invocations ────────────────────────────────────────
            // Macros are opaque to us; treat as a single operand
            Expr::Macro(expr_macro) => {
                if let Some(ident) = expr_macro.mac.path.get_ident() {
                    self.events.push(QualitasEvent::Operand(OperandEvent {
                        name: format!("{ident}!"),
                    }));
                }
            }

            // ── Catch-all for other expression types ─────────────────────
            _ => {}
        }
    }

    fn visit_if(&mut self, expr_if: &ExprIf) {
        let has_else = expr_if.else_branch.is_some();
        let else_is_if = expr_if
            .else_branch
            .as_ref()
            .is_some_and(|(_, e)| matches!(e.as_ref(), Expr::If(_)));

        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::If,
                has_else,
                else_is_if,
            }));

        // Visit condition
        self.visit_expr_inner(&expr_if.cond);

        // Nesting for the then-block
        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        self.visit_block_stmts(&expr_if.then_branch);

        if let Some((_, else_branch)) = &expr_if.else_branch {
            match else_branch.as_ref() {
                Expr::If(inner_if) => {
                    // else-if chain: +1 flat, then recurse
                    self.events
                        .push(QualitasEvent::LogicOp(LogicOpEvent::Ternary));
                    self.visit_if(inner_if);
                }
                Expr::Block(block) => {
                    self.visit_block_stmts(&block.block);
                }
                other => {
                    self.visit_expr_inner(other);
                }
            }
        }

        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_binary(&mut self, expr_bin: &ExprBinary) {
        // Check for logical operators (CFC events)
        match &expr_bin.op {
            BinOp::And(_) => {
                self.events.push(QualitasEvent::LogicOp(LogicOpEvent::And));
            }
            BinOp::Or(_) => {
                self.events.push(QualitasEvent::LogicOp(LogicOpEvent::Or));
            }
            _ => {}
        }

        // DCI: all binary operators
        let op_name = match &expr_bin.op {
            BinOp::Add(_) => "+",
            BinOp::Sub(_) => "-",
            BinOp::Mul(_) => "*",
            BinOp::Div(_) => "/",
            BinOp::Rem(_) => "%",
            BinOp::And(_) => "&&",
            BinOp::Or(_) => "||",
            BinOp::BitXor(_) => "^",
            BinOp::BitAnd(_) => "&",
            BinOp::BitOr(_) => "|",
            BinOp::Shl(_) => "<<",
            BinOp::Shr(_) => ">>",
            BinOp::Eq(_) => "==",
            BinOp::Lt(_) => "<",
            BinOp::Le(_) => "<=",
            BinOp::Ne(_) => "!=",
            BinOp::Ge(_) => ">=",
            BinOp::Gt(_) => ">",
            BinOp::AddAssign(_) => "+=",
            BinOp::SubAssign(_) => "-=",
            BinOp::MulAssign(_) => "*=",
            BinOp::DivAssign(_) => "/=",
            BinOp::RemAssign(_) => "%=",
            BinOp::BitXorAssign(_) => "^=",
            BinOp::BitAndAssign(_) => "&=",
            BinOp::BitOrAssign(_) => "|=",
            BinOp::ShlAssign(_) => "<<=",
            BinOp::ShrAssign(_) => ">>=",
            _ => "?op",
        };
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: op_name.to_string(),
        }));

        self.visit_expr_inner(&expr_bin.left);
        self.visit_expr_inner(&expr_bin.right);
    }

    fn visit_call(&mut self, expr_call: &ExprCall) {
        // Recursive self-call detection
        if let Expr::Path(p) = expr_call.func.as_ref() {
            if let Some(ident) = p.path.get_ident() {
                let name = ident.to_string();
                if name == self.fn_name {
                    self.events.push(QualitasEvent::RecursiveCall);
                }
            }

            // Detect qualified calls like `module::function()`
            if p.path.segments.len() >= 2 {
                let segs: Vec<String> = p
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect();
                let object = &segs[..segs.len() - 1].join("::");
                let method = &segs[segs.len() - 1];
                if self.imported_names.contains(&segs[0])
                    || self.imported_names.contains(object.as_str())
                {
                    self.events.push(QualitasEvent::ApiCall(ApiCallEvent {
                        object: object.clone(),
                        method: method.clone(),
                    }));
                }
            }
        }

        // Visit function expression
        self.visit_expr_inner(&expr_call.func);
        // Visit arguments
        for arg in &expr_call.args {
            self.visit_expr_inner(arg);
        }
    }

    fn visit_method_call(&mut self, expr_method: &ExprMethodCall) {
        let method_name = expr_method.method.to_string();

        // Detect .await-like patterns and common async spawns
        if method_name == "spawn" || method_name == "spawn_blocking" {
            self.events
                .push(QualitasEvent::AsyncComplexity(AsyncEvent::Spawn));
        }

        // DC: method calls on imported objects
        if let Expr::Path(p) = expr_method.receiver.as_ref() {
            if let Some(ident) = p.path.get_ident() {
                let obj_name = ident.to_string();
                if self.imported_names.contains(&obj_name) {
                    self.events.push(QualitasEvent::ApiCall(ApiCallEvent {
                        object: obj_name,
                        method: method_name.clone(),
                    }));
                }
            }
        }

        // Visit receiver
        self.visit_expr_inner(&expr_method.receiver);
        // Visit arguments
        for arg in &expr_method.args {
            self.visit_expr_inner(arg);
        }
    }

    fn visit_match_arm(&mut self, arm: &Arm) {
        // Each arm is a branch point
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ContextManager,
                has_else: false,
                else_is_if: false,
            }));
        // Emit bindings from the arm pattern
        self.emit_pattern_bindings(&arm.pat);
        // Visit guard condition
        if let Some((_, guard)) = &arm.guard {
            self.visit_expr_inner(guard);
        }
        // Visit arm body
        self.visit_expr_inner(&arm.body);
    }

    fn visit_block_stmts(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Local(local) => {
                // let binding
                self.emit_pattern_bindings(&local.pat);
                if let Some(init) = &local.init {
                    self.visit_expr_inner(&init.expr);
                    // Diverge expression (else branch in let-else)
                    if let Some((_, diverge)) = &init.diverge {
                        self.visit_expr_inner(diverge);
                    }
                }
            }
            Stmt::Expr(expr, _) => {
                self.visit_expr_inner(expr);
            }
            Stmt::Item(item) => {
                // Nested function definitions inside a function body
                if let Item::Fn(inner_fn) = item {
                    self.events.push(QualitasEvent::NestedFunctionEnter);
                    self.events.push(QualitasEvent::NestingEnter);
                    self.nesting_depth += 1;
                    self.visit_block_stmts(&inner_fn.block);
                    self.nesting_depth -= 1;
                    self.events.push(QualitasEvent::NestingExit);
                    self.events.push(QualitasEvent::NestedFunctionExit);
                }
            }
            Stmt::Macro(stmt_macro) => {
                if let Some(ident) = stmt_macro.mac.path.get_ident() {
                    self.events.push(QualitasEvent::Operand(OperandEvent {
                        name: format!("{ident}!"),
                    }));
                }
            }
        }
    }
}

// We don't use syn's Visit trait for the body emitter — we use manual recursion
// for precise control over event emission order.
impl<'ast> Visit<'ast> for RustBodyEventEmitter<'_> {
    fn visit_block(&mut self, block: &'ast Block) {
        self.visit_block_stmts(block);
    }
}
