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

    fn test_patterns(&self) -> &[&str] {
        &["_test.rs", "_tests.rs", "tests/"]
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
fn line_col_to_byte(source: &str, line: usize, col: usize) -> u32 {
    let mut current_line = 1;
    let mut byte_offset = 0;
    for (i, ch) in source.char_indices() {
        if current_line == line {
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

fn use_tree_source(tree: &UseTree) -> String {
    match tree {
        UseTree::Path(p) => p.ident.to_string(),
        UseTree::Name(n) => n.ident.to_string(),
        UseTree::Rename(r) => r.ident.to_string(),
        UseTree::Glob(_) => "*".to_string(),
        UseTree::Group(_) => "(group)".to_string(),
    }
}

// ─── Pure mapping: BinOp → operator name ─────────────────────────────────────

fn binary_op_name(op: &BinOp) -> &'static str {
    binary_op_name_simple(op)
        .or_else(|| binary_op_name_comparison(op))
        .or_else(|| binary_op_name_assign(op))
        .unwrap_or("?op")
}

/// Arithmetic and logic binary operators.
fn binary_op_name_simple(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::Add(_) => Some("+"),
        BinOp::Sub(_) => Some("-"),
        BinOp::Mul(_) => Some("*"),
        BinOp::Div(_) => Some("/"),
        BinOp::Rem(_) => Some("%"),
        BinOp::And(_) => Some("&&"),
        BinOp::Or(_) => Some("||"),
        BinOp::BitXor(_) => Some("^"),
        BinOp::BitAnd(_) => Some("&"),
        BinOp::BitOr(_) => Some("|"),
        BinOp::Shl(_) => Some("<<"),
        BinOp::Shr(_) => Some(">>"),
        _ => None,
    }
}

/// Comparison binary operators.
fn binary_op_name_comparison(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::Eq(_) => Some("=="),
        BinOp::Lt(_) => Some("<"),
        BinOp::Le(_) => Some("<="),
        BinOp::Ne(_) => Some("!="),
        BinOp::Ge(_) => Some(">="),
        BinOp::Gt(_) => Some(">"),
        _ => None,
    }
}

/// Compound assignment binary operators.
fn binary_op_name_assign(op: &BinOp) -> Option<&'static str> {
    match op {
        BinOp::AddAssign(_) => Some("+="),
        BinOp::SubAssign(_) => Some("-="),
        BinOp::MulAssign(_) => Some("*="),
        BinOp::DivAssign(_) => Some("/="),
        BinOp::RemAssign(_) => Some("%="),
        BinOp::BitXorAssign(_) => Some("^="),
        BinOp::BitAndAssign(_) => Some("&="),
        BinOp::BitOrAssign(_) => Some("|="),
        BinOp::ShlAssign(_) => Some("<<="),
        BinOp::ShrAssign(_) => Some(">>="),
        _ => None,
    }
}

fn unary_op_name(op: &UnOp) -> &'static str {
    match op {
        UnOp::Not(_) => "!",
        UnOp::Neg(_) => "-",
        UnOp::Deref(_) => "*",
        _ => "?unary",
    }
}

// ─── Top-level extractor ────────────────────────────────────────────────────

struct RustExtractor<'src> {
    source: &'src str,
    functions: Vec<FunctionExtraction>,
    classes: Vec<ClassExtraction>,
    imports: Vec<ImportRecord>,
    imported_names: HashSet<String>,
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
            is_generator: false,
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
        let fe = FunctionExtraction {
            byte_start: span_to_byte_start(self.source, node.sig.fn_token.span),
            byte_end: span_to_byte_end(self.source, node.block.brace_token.span.close()),
            start_line: span_to_line(node.sig.fn_token.span),
            end_line: span_to_end_line(node.block.brace_token.span.close()),
            ..fe
        };
        self.push_fn(fe);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
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

        for item in &node.items {
            if let ImplItem::Fn(method) = item {
                self.visit_impl_item_fn(method);
            }
        }

        self.impl_stack.pop();
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        let name = node.sig.ident.to_string();
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

    fn emit_pattern_bindings(&mut self, pat: &Pat) {
        match pat {
            Pat::Ident(pi) => {
                self.events
                    .push(QualitasEvent::IdentDeclaration(IdentEvent {
                        name: pi.ident.to_string(),
                        byte_offset: span_to_byte_start(self.source, pi.ident.span()),
                    }));
            }
            Pat::Reference(pr) => self.emit_pattern_bindings(&pr.pat),
            Pat::Type(pt) => self.emit_pattern_bindings(&pt.pat),
            _ => self.emit_compound_pattern_bindings(pat),
        }
    }

    fn emit_compound_pattern_bindings(&mut self, pat: &Pat) {
        let elems: Option<&syn::punctuated::Punctuated<Pat, _>> = match pat {
            Pat::Tuple(pt) => Some(&pt.elems),
            Pat::TupleStruct(pts) => Some(&pts.elems),
            Pat::Slice(ps) => Some(&ps.elems),
            _ => None,
        };
        if let Some(elems) = elems {
            for elem in elems {
                self.emit_pattern_bindings(elem);
            }
            return;
        }
        match pat {
            Pat::Struct(ps) => {
                for field in &ps.fields {
                    self.emit_pattern_bindings(&field.pat);
                }
            }
            Pat::Or(po) => {
                for case in &po.cases {
                    self.emit_pattern_bindings(case);
                }
            }
            _ => {}
        }
    }

    /// Helper: push a nesting block around a loop/match body.
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

    /// Helper: push a nested function boundary (closure/async block).
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

    // ── Expression dispatcher ────────────────────────────────────────────

    fn visit_expr_inner(&mut self, expr: &Expr) {
        if self.emit_control_flow(expr) {
            return;
        }
        if self.emit_operators(expr) {
            return;
        }
        if self.emit_special_ops(expr) {
            return;
        }
        if self.emit_flow_control(expr) {
            return;
        }
        if self.emit_calls(expr) {
            return;
        }
        if self.emit_async_closures(expr) {
            return;
        }
        if self.emit_operands(expr) {
            return;
        }
        self.emit_containers(expr);
    }

    // ── Control flow: if, for, while, loop, match ────────────────────────

    fn emit_control_flow(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::If(expr_if) => {
                self.visit_if(expr_if);
                true
            }
            Expr::ForLoop(expr_for) => {
                self.emit_control_flow_event(ControlFlowKind::ForOf);
                self.emit_pattern_bindings(&expr_for.pat);
                self.visit_expr_inner(&expr_for.expr);
                self.emit_nesting_block(|s| s.visit_block_stmts(&expr_for.body));
                true
            }
            Expr::While(expr_while) => {
                self.emit_control_flow_event(ControlFlowKind::While);
                self.visit_expr_inner(&expr_while.cond);
                self.emit_nesting_block(|s| s.visit_block_stmts(&expr_while.body));
                true
            }
            Expr::Loop(expr_loop) => {
                self.emit_control_flow_event(ControlFlowKind::While);
                self.emit_nesting_block(|s| s.visit_block_stmts(&expr_loop.body));
                true
            }
            Expr::Match(expr_match) => {
                self.emit_control_flow_event(ControlFlowKind::PatternMatch);
                self.visit_expr_inner(&expr_match.expr);
                self.emit_nesting_block(|s| {
                    for arm in &expr_match.arms {
                        s.visit_match_arm(arm);
                    }
                });
                true
            }
            _ => false,
        }
    }

    fn emit_control_flow_event(&mut self, kind: ControlFlowKind) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind,
                has_else: false,
                else_is_if: false,
            }));
    }

    // ── Operators: binary, unary, reference ──────────────────────────────

    fn emit_operators(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::Binary(expr_bin) => {
                self.visit_binary(expr_bin);
                true
            }
            Expr::Unary(expr_un) => {
                self.emit_operator(unary_op_name(&expr_un.op));
                self.visit_expr_inner(&expr_un.expr);
                true
            }
            Expr::Reference(expr_ref) => {
                let op = if expr_ref.mutability.is_some() {
                    "&mut"
                } else {
                    "&"
                };
                self.emit_operator(op);
                self.visit_expr_inner(&expr_ref.expr);
                true
            }
            _ => false,
        }
    }

    fn emit_operator(&mut self, name: &str) {
        self.events.push(QualitasEvent::Operator(OperatorEvent {
            name: name.to_string(),
        }));
    }

    // ── Special ops: assign, range, cast, try ────────────────────────────

    fn emit_special_ops(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::Assign(a) => {
                self.emit_operator("=");
                self.visit_expr_inner(&a.left);
                self.visit_expr_inner(&a.right);
                true
            }
            Expr::Range(r) => {
                self.emit_operator("..");
                if let Some(start) = &r.start {
                    self.visit_expr_inner(start);
                }
                if let Some(end) = &r.end {
                    self.visit_expr_inner(end);
                }
                true
            }
            Expr::Cast(c) => {
                self.emit_operator("as");
                self.visit_expr_inner(&c.expr);
                true
            }
            Expr::Try(t) => {
                self.events.push(QualitasEvent::ReturnStatement);
                self.emit_operator("?");
                self.visit_expr_inner(&t.expr);
                true
            }
            _ => false,
        }
    }

    // ── Flow control: return, break, continue ────────────────────────────

    fn emit_flow_control(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::Return(ret) => {
                self.events.push(QualitasEvent::ReturnStatement);
                if let Some(val) = &ret.expr {
                    self.visit_expr_inner(val);
                }
                true
            }
            Expr::Break(brk) => {
                if brk.label.is_some() {
                    self.events.push(QualitasEvent::LabeledFlow);
                }
                if let Some(val) = &brk.expr {
                    self.visit_expr_inner(val);
                }
                true
            }
            Expr::Continue(cont) => {
                if cont.label.is_some() {
                    self.events.push(QualitasEvent::LabeledFlow);
                }
                true
            }
            _ => false,
        }
    }

    // ── Calls: function calls, method calls ──────────────────────────────

    fn emit_calls(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::Call(c) => {
                self.visit_call(c);
                true
            }
            Expr::MethodCall(m) => {
                self.visit_method_call(m);
                true
            }
            _ => false,
        }
    }

    // ── Async & closures ─────────────────────────────────────────────────

    fn emit_async_closures(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::Closure(c) => {
                if self.nesting_depth > 0 {
                    self.events.push(QualitasEvent::NestedCallback);
                }
                self.emit_nested_fn_block(|s| s.visit_expr_inner(&c.body));
                true
            }
            Expr::Async(a) => {
                self.events
                    .push(QualitasEvent::AsyncComplexity(AsyncEvent::Spawn));
                self.emit_nested_fn_block(|s| s.visit_block_stmts(&a.block));
                true
            }
            Expr::Await(aw) => {
                if self.nesting_depth > 1 {
                    self.events
                        .push(QualitasEvent::AsyncComplexity(AsyncEvent::Await));
                }
                self.visit_expr_inner(&aw.base);
                true
            }
            _ => false,
        }
    }

    // ── Operands: identifiers, paths, literals ───────────────────────────

    fn emit_operands(&mut self, expr: &Expr) -> bool {
        match expr {
            Expr::Path(p) => {
                self.emit_path_operand(p);
                true
            }
            Expr::Lit(lit) => {
                self.emit_literal_operand(lit);
                true
            }
            _ => false,
        }
    }

    fn emit_path_operand(&mut self, expr_path: &syn::ExprPath) {
        if let Some(ident) = expr_path.path.get_ident() {
            let name = ident.to_string();
            self.events
                .push(QualitasEvent::Operand(OperandEvent { name: name.clone() }));
            self.events.push(QualitasEvent::IdentReference(IdentEvent {
                name,
                byte_offset: span_to_byte_start(self.source, ident.span()),
            }));
        } else if expr_path.path.segments.len() > 1 {
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

    fn emit_literal_operand(&mut self, expr_lit: &syn::ExprLit) {
        let name = match &expr_lit.lit {
            Lit::Int(i) => i.to_string(),
            Lit::Float(f) => f.to_string(),
            Lit::Str(s) => {
                let val = s.value();
                val[..val.len().min(32)].to_string()
            }
            Lit::Bool(b) => if b.value { "true" } else { "false" }.to_string(),
            Lit::Char(c) => c.value().to_string(),
            Lit::Byte(b) => b.value().to_string(),
            _ => return,
        };
        self.events
            .push(QualitasEvent::Operand(OperandEvent { name }));
    }

    // ── Containers & transparent wrappers ────────────────────────────────

    fn emit_containers(&mut self, expr: &Expr) {
        match expr {
            // Simple delegations
            Expr::Block(b) => self.visit_block_stmts(&b.block),
            Expr::Unsafe(u) => self.visit_block_stmts(&u.block),
            Expr::Field(f) => self.visit_expr_inner(&f.base),
            Expr::Paren(p) => self.visit_expr_inner(&p.expr),
            Expr::Group(g) => self.visit_expr_inner(&g.expr),
            // Compound containers
            _ => self.emit_compound_containers(expr),
        }
    }

    fn emit_compound_containers(&mut self, expr: &Expr) {
        match expr {
            Expr::Index(idx) => self.emit_index_expr(idx),
            Expr::Tuple(t) => self.visit_expr_list(&t.elems),
            Expr::Array(a) => self.visit_expr_list(&a.elems),
            Expr::Struct(s) => self.emit_struct_expr(s),
            Expr::Let(l) => self.emit_let_expr(l),
            Expr::Repeat(r) => self.emit_repeat_expr(r),
            Expr::Macro(m) => self.emit_macro_operand(&m.mac),
            _ => {}
        }
    }

    fn emit_index_expr(&mut self, idx: &syn::ExprIndex) {
        self.emit_operator("[]");
        self.visit_expr_inner(&idx.expr);
        self.visit_expr_inner(&idx.index);
    }

    fn emit_struct_expr(&mut self, s: &syn::ExprStruct) {
        for field in &s.fields {
            self.visit_expr_inner(&field.expr);
        }
        if let Some(rest) = &s.rest {
            self.visit_expr_inner(rest);
        }
    }

    fn emit_let_expr(&mut self, l: &syn::ExprLet) {
        self.emit_pattern_bindings(&l.pat);
        self.visit_expr_inner(&l.expr);
    }

    fn emit_repeat_expr(&mut self, r: &syn::ExprRepeat) {
        self.visit_expr_inner(&r.expr);
        self.visit_expr_inner(&r.len);
    }

    fn emit_macro_operand(&mut self, mac: &syn::Macro) {
        if let Some(ident) = mac.path.get_ident() {
            self.events.push(QualitasEvent::Operand(OperandEvent {
                name: format!("{ident}!"),
            }));
        }
    }

    fn visit_expr_list(&mut self, elems: &syn::punctuated::Punctuated<Expr, syn::Token![,]>) {
        for elem in elems {
            self.visit_expr_inner(elem);
        }
    }

    // ── If expression (with else-if chain handling) ──────────────────────

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

        self.visit_expr_inner(&expr_if.cond);

        self.events.push(QualitasEvent::NestingEnter);
        self.nesting_depth += 1;
        self.visit_block_stmts(&expr_if.then_branch);

        if let Some((_, else_branch)) = &expr_if.else_branch {
            self.visit_else_branch(else_branch);
        }

        self.nesting_depth -= 1;
        self.events.push(QualitasEvent::NestingExit);
    }

    fn visit_else_branch(&mut self, branch: &Expr) {
        match branch {
            Expr::If(inner_if) => {
                self.events
                    .push(QualitasEvent::LogicOp(LogicOpEvent::Ternary));
                self.visit_if(inner_if);
            }
            Expr::Block(block) => self.visit_block_stmts(&block.block),
            other => self.visit_expr_inner(other),
        }
    }

    // ── Binary expression ────────────────────────────────────────────────

    fn visit_binary(&mut self, expr_bin: &ExprBinary) {
        match &expr_bin.op {
            BinOp::And(_) => self.events.push(QualitasEvent::LogicOp(LogicOpEvent::And)),
            BinOp::Or(_) => self.events.push(QualitasEvent::LogicOp(LogicOpEvent::Or)),
            _ => {}
        }

        self.emit_operator(binary_op_name(&expr_bin.op));
        self.visit_expr_inner(&expr_bin.left);
        self.visit_expr_inner(&expr_bin.right);
    }

    // ── Function & method calls ──────────────────────────────────────────

    fn visit_call(&mut self, expr_call: &ExprCall) {
        self.detect_call_patterns(expr_call);
        self.visit_expr_inner(&expr_call.func);
        for arg in &expr_call.args {
            self.visit_expr_inner(arg);
        }
    }

    fn detect_call_patterns(&mut self, expr_call: &ExprCall) {
        let Expr::Path(p) = expr_call.func.as_ref() else {
            return;
        };
        if let Some(ident) = p.path.get_ident() {
            if *ident == self.fn_name {
                self.events.push(QualitasEvent::RecursiveCall);
            }
        }
        if p.path.segments.len() >= 2 {
            self.detect_qualified_api_call(p);
        }
    }

    fn detect_qualified_api_call(&mut self, p: &syn::ExprPath) {
        let segs: Vec<String> = p
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let object = segs[..segs.len() - 1].join("::");
        let method = &segs[segs.len() - 1];
        if self.imported_names.contains(&segs[0]) || self.imported_names.contains(object.as_str()) {
            self.events.push(QualitasEvent::ApiCall(ApiCallEvent {
                object,
                method: method.clone(),
            }));
        }
    }

    fn visit_method_call(&mut self, expr_method: &ExprMethodCall) {
        let method_name = expr_method.method.to_string();

        if method_name == "spawn" || method_name == "spawn_blocking" {
            self.events
                .push(QualitasEvent::AsyncComplexity(AsyncEvent::Spawn));
        }

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

        self.visit_expr_inner(&expr_method.receiver);
        for arg in &expr_method.args {
            self.visit_expr_inner(arg);
        }
    }

    // ── Match arm ────────────────────────────────────────────────────────

    fn visit_match_arm(&mut self, arm: &Arm) {
        self.events
            .push(QualitasEvent::ControlFlow(ControlFlowEvent {
                kind: ControlFlowKind::ContextManager,
                has_else: false,
                else_is_if: false,
            }));
        self.emit_pattern_bindings(&arm.pat);
        if let Some((_, guard)) = &arm.guard {
            self.visit_expr_inner(guard);
        }
        self.visit_expr_inner(&arm.body);
    }

    // ── Block & statement visitors ───────────────────────────────────────

    fn visit_block_stmts(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Local(local) => {
                self.emit_pattern_bindings(&local.pat);
                self.visit_local_init(local);
            }
            Stmt::Expr(expr, _) => self.visit_expr_inner(expr),
            Stmt::Item(item) => self.visit_nested_item(item),
            Stmt::Macro(m) => self.emit_macro_operand(&m.mac),
        }
    }

    fn visit_local_init(&mut self, local: &syn::Local) {
        if let Some(init) = &local.init {
            self.visit_expr_inner(&init.expr);
            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr_inner(diverge);
            }
        }
    }

    fn visit_nested_item(&mut self, item: &Item) {
        if let Item::Fn(inner_fn) = item {
            self.emit_nested_fn_block(|s| s.visit_block_stmts(&inner_fn.block));
        }
    }
}

impl<'ast> Visit<'ast> for RustBodyEventEmitter<'_> {
    fn visit_block(&mut self, block: &'ast Block) {
        self.visit_block_stmts(block);
    }
}
