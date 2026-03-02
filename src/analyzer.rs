/// Main analysis orchestrator.
///
/// Ties together: parser → 5 metrics → scorer → report assembly.
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast::Visit;
use oxc_parser::Parser;
use oxc_span::SourceType;
use oxc_syntax::scope::ScopeFlags;
use std::collections::HashSet;

use crate::metrics::{
    cognitive_flow::analyze_cfc_body,
    data_complexity::analyze_dci_body,
    dependencies::{analyze_file_dependencies, analyze_function_dependencies, collect_imported_names},
    identifier_refs::analyze_irc_body,
    structural::{analyze_structural_body, compute_sm_raw},
};
use crate::parser::ast::{byte_to_line, parse_source};
use crate::scorer::{
    composite::{aggregate_scores, compute_score},
    thresholds::{generate_flags, grade_from_score},
};
use crate::types::*;

pub fn analyze_source_str(
    source: &str,
    file_path: &str,
    options: &AnalysisOptions,
) -> Result<FileQualityReport, String> {
    let parsed = parse_source(source, file_path)?;

    let profile = options.profile.as_deref();
    let weights_ref = options.weights.as_ref();
    let refactoring_threshold = options
        .refactoring_threshold
        .unwrap_or(crate::constants::DEFAULT_REFACTORING_THRESHOLD);

    let file_deps = analyze_file_dependencies(&parsed.import_records);
    let imported_names: HashSet<String> = collect_imported_names(&parsed.import_records);

    // Re-parse to get AST with allocator for metric analysis
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(file_path)
        .unwrap_or_else(|_| SourceType::default().with_typescript(true));
    let parse_result = Parser::new(&allocator, source, source_type).parse();
    let program = &parse_result.program;

    // Collect all function bodies in one pass
    let mut fn_collector = FnBodyCollector::new(source, &imported_names);
    fn_collector.visit_program(program);

    // Capture counts before partial moves below
    let fn_count = fn_collector.fn_count;
    let class_count = fn_collector.class_count;

    // Build function reports
    let function_reports: Vec<FunctionQualityReport> = fn_collector
        .functions
        .into_iter()
        .map(|fi| build_fn_report(fi, profile, weights_ref, refactoring_threshold))
        .collect();

    // Build class reports
    let class_reports: Vec<ClassQualityReport> = fn_collector
        .classes
        .into_iter()
        .map(|ci| build_class_report(ci, profile, weights_ref, refactoring_threshold))
        .collect();

    // File-level score
    let mut all_scores: Vec<(f64, u32)> = function_reports
        .iter()
        .map(|r| (r.score, r.metrics.structural.loc.max(1)))
        .collect();
    for cr in &class_reports {
        for mr in &cr.methods {
            all_scores.push((mr.score, mr.metrics.structural.loc.max(1)));
        }
    }

    let file_score = if all_scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&all_scores)
    };

    let grade = grade_from_score(file_score, profile);
    let needs_refactoring = file_score < refactoring_threshold;
    let flagged_fn_count = function_reports
        .iter()
        .filter(|r| r.needs_refactoring)
        .count() as u32;

    Ok(FileQualityReport {
        file_path: file_path.to_string(),
        score: file_score,
        grade,
        needs_refactoring,
        flags: vec![],
        functions: function_reports,
        classes: class_reports,
        file_dependencies: file_deps,
        total_lines: source.lines().count() as u32,
        function_count: fn_count,
        class_count,
        flagged_function_count: flagged_fn_count,
    })
}

// ─── Collected function/class data for metric analysis ────────────────────────

struct CollectedFunction {
    name: String,
    inferred_name: Option<String>,
    /// 1-based line number of function start
    start_line: u32,
    /// 1-based line number of function end
    end_line: u32,
    is_async: bool,
    is_generator: bool,
    cfc: CognitiveFlowResult,
    dci: DataComplexityResult,
    irc: IdentifierRefResult,
    dc: DependencyCouplingResult,
    sm: StructuralResult,
}

struct CollectedClass {
    name: String,
    start_line: u32,
    end_line: u32,
    methods: Vec<CollectedFunction>,
}

struct FnBodyCollector<'src> {
    source: &'src str,
    imported_names: &'src HashSet<String>,
    functions: Vec<CollectedFunction>,
    classes: Vec<CollectedClass>,
    fn_count: u32,
    class_count: u32,
    class_stack: Vec<usize>,
}

impl<'src> FnBodyCollector<'src> {
    fn new(source: &'src str, imported_names: &'src HashSet<String>) -> Self {
        Self {
            source,
            imported_names,
            functions: Vec::new(),
            classes: Vec::new(),
            fn_count: 0,
            class_count: 0,
            class_stack: Vec::new(),
        }
    }

    /// Analyze a `Function` node (declaration or expression) and collect it.
    fn analyze_fn(
        &self,
        func: &Function<'_>,
        name: &str,
        inferred_name: Option<String>,
    ) -> CollectedFunction {
        let param_count = func.params.items.len() as u32;
        let start_line = byte_to_line(self.source, func.span.start);
        let end_line = byte_to_line(self.source, func.span.end);

        let (cfc, dci, irc, dc, sm) = if let Some(body) = &func.body {
            let cfc = analyze_cfc_body(body, name);
            let dci = analyze_dci_body(body);
            let irc = analyze_irc_body(body, self.source);
            let dc = analyze_function_dependencies(body, self.imported_names);
            let sm = analyze_structural_body(
                body,
                self.source,
                func.span.start,
                func.span.end,
                param_count,
            );
            (cfc, dci, irc, dc, sm)
        } else {
            zero_metrics(param_count)
        };

        CollectedFunction {
            name: name.to_string(),
            inferred_name,
            start_line,
            end_line,
            is_async: func.r#async,
            is_generator: func.generator,
            cfc,
            dci,
            irc,
            dc,
            sm,
        }
    }

    /// Analyze an `ArrowFunctionExpression` node and collect it.
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
        let start_line = byte_to_line(self.source, arrow.span.start);
        let end_line = byte_to_line(self.source, arrow.span.end);
        let body: &FunctionBody = &arrow.body;
        let cfc = analyze_cfc_body(body, &name);
        let dci = analyze_dci_body(body);
        let irc = analyze_irc_body(body, self.source);
        let dc = analyze_function_dependencies(body, self.imported_names);
        let sm = analyze_structural_body(
            body,
            self.source,
            arrow.span.start,
            arrow.span.end,
            param_count,
        );
        self.push_fn(CollectedFunction {
            name,
            inferred_name,
            start_line,
            end_line,
            is_async: arrow.r#async,
            is_generator: false,
            cfc,
            dci,
            irc,
            dc,
            sm,
        });
    }

    fn push_fn(&mut self, cf: CollectedFunction) {
        if let Some(&ci) = self.class_stack.last() {
            self.classes[ci].methods.push(cf);
        } else {
            self.functions.push(cf);
        }
        self.fn_count += 1;
    }
}

// ─── Visitor implementation ────────────────────────────────────────────────────

impl<'a> Visit<'a> for FnBodyCollector<'_> {
    /// `function foo() {}` and class method `foo() {}` (FunctionDeclaration /
    /// FunctionExpression — both share the `Function` AST node).
    fn visit_function(&mut self, func: &Function<'a>, _flags: ScopeFlags) {
        let name = func
            .id
            .as_ref()
            .map(|id| id.name.as_str())
            .unwrap_or("(anonymous)")
            .to_string();
        let cf = self.analyze_fn(func, &name, None);
        self.push_fn(cf);
        // Don't recurse into nested functions — they're collected at top level only
    }

    /// `const foo = () => {}` and `const foo = function() {}`
    /// Also recurses for other initialisers (object literals, etc.) so that
    /// nested patterns handled by other overrides are still visited.
    fn visit_variable_declarator(&mut self, decl: &VariableDeclarator<'a>) {
        let name = match &decl.id.kind {
            BindingPatternKind::BindingIdentifier(id) => id.name.to_string(),
            _ => {
                use oxc_ast::visit::walk;
                walk::walk_variable_declarator(self, decl);
                return;
            }
        };

        if let Some(init) = &decl.init {
            match init {
                Expression::FunctionExpression(f) => {
                    let inferred = Some(format!("const {name} = function"));
                    let cf = self.analyze_fn(f, &name, inferred);
                    self.push_fn(cf);
                    return; // don't recurse
                }
                Expression::ArrowFunctionExpression(arrow) => {
                    let inferred = Some(format!("const {name} = "));
                    self.collect_arrow(arrow, name, inferred);
                    return; // don't recurse
                }
                _ => {}
            }
        }

        use oxc_ast::visit::walk;
        walk::walk_variable_declarator(self, decl);
    }

    /// `{ method: () => {} }` and `{ helper: function() {} }`
    ///
    /// Iterates properties explicitly so each arrow/function value gets the
    /// property key as its name, and we avoid recursing into collected bodies.
    fn visit_object_expression(&mut self, obj: &ObjectExpression<'a>) {
        for prop in &obj.properties {
            if let ObjectPropertyKind::ObjectProperty(p) = prop {
                let name = property_key_name(&p.key);
                match &p.value {
                    Expression::ArrowFunctionExpression(arrow) => {
                        self.collect_arrow(arrow, name, None);
                        // don't recurse into this body
                    }
                    Expression::FunctionExpression(f) => {
                        let cf = self.analyze_fn(f, &name, None);
                        self.push_fn(cf);
                        // don't recurse into this body
                    }
                    other => {
                        // For any other value (nested object, array, call, etc.)
                        // continue the normal visitor dispatch so nested patterns work
                        self.visit_expression(other);
                    }
                }
            }
            // SpreadElement / SpreadProperty: no function definitions directly
        }
    }

    /// `export default () => {}` and `export default function() {}`
    fn visit_export_default_declaration(&mut self, decl: &ExportDefaultDeclaration<'a>) {
        use oxc_ast::visit::walk;
        match &decl.declaration {
            ExportDefaultDeclarationKind::ArrowFunctionExpression(arrow) => {
                self.collect_arrow(
                    arrow,
                    "(default)".to_string(),
                    Some("export default ".to_string()),
                );
            }
            ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                let name = f
                    .id
                    .as_ref()
                    .map(|id| id.name.as_str())
                    .unwrap_or("(default)")
                    .to_string();
                let cf = self.analyze_fn(f, &name, Some("export default ".to_string()));
                self.push_fn(cf);
            }
            _ => {
                walk::walk_export_default_declaration(self, decl);
            }
        }
    }

    /// `class Foo { method(args) {} }` — class method definitions.
    ///
    /// The `Function` node inside a `MethodDefinition` has `id = None`, so the
    /// name must come from `MethodDefinition.key`.  By overriding here (without
    /// recursing) we avoid `visit_function` giving the method an "(anonymous)" name.
    fn visit_method_definition(&mut self, method: &MethodDefinition<'a>) {
        let name = property_key_name(&method.key);
        let cf = self.analyze_fn(&method.value, &name, None);
        self.push_fn(cf);
        // Don't recurse — prevents visit_function from re-collecting with "(anonymous)"
    }

    /// `class Foo { method = () => {} }` — class property arrows.
    /// Regular class methods (`foo() {}`) are handled by `visit_method_definition`.
    fn visit_property_definition(&mut self, prop: &PropertyDefinition<'a>) {
        use oxc_ast::visit::walk;
        if let Some(Expression::ArrowFunctionExpression(arrow)) = &prop.value {
            let name = property_key_name(&prop.key);
            self.collect_arrow(arrow, name, None);
            // don't recurse into the arrow body
        } else {
            walk::walk_property_definition(self, prop);
        }
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        let name = class
            .id
            .as_ref()
            .map(|id| id.name.as_str())
            .unwrap_or("(anonymous class)")
            .to_string();
        let start_line = byte_to_line(self.source, class.span.start);
        let end_line = byte_to_line(self.source, class.span.end);
        let cc = CollectedClass {
            name,
            start_line,
            end_line,
            methods: Vec::new(),
        };
        let idx = self.classes.len();
        self.classes.push(cc);
        self.class_stack.push(idx);
        self.class_count += 1;

        use oxc_ast::visit::walk;
        walk::walk_class(self, class);

        self.class_stack.pop();
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

/// All-zero metric results for abstract / body-less functions.
fn zero_metrics(param_count: u32) -> (
    CognitiveFlowResult,
    DataComplexityResult,
    IdentifierRefResult,
    DependencyCouplingResult,
    StructuralResult,
) {
    (
        CognitiveFlowResult {
            score: 0,
            nesting_penalty: 0,
            base_increments: 0,
            async_penalty: 0,
            max_nesting_depth: 0,
        },
        DataComplexityResult {
            halstead: HalsteadCounts {
                distinct_operators: 0,
                distinct_operands: 0,
                total_operators: 0,
                total_operands: 0,
            },
            difficulty: 0.0,
            volume: 0.0,
            effort: 0.0,
            raw_score: 0.0,
        },
        IdentifierRefResult { total_irc: 0.0, hotspots: vec![] },
        DependencyCouplingResult {
            import_count: 0,
            distinct_sources: 0,
            external_ratio: 0.0,
            external_packages: vec![],
            internal_modules: vec![],
            distinct_api_calls: 0,
            closure_captures: 0,
            raw_score: 0.0,
        },
        StructuralResult {
            loc: 0,
            total_lines: 0,
            parameter_count: param_count,
            max_nesting_depth: 0,
            return_count: 0,
            method_count: None,
            raw_score: 0.0,
        },
    )
}

// ─── Report assembly ──────────────────────────────────────────────────────────

fn build_fn_report(
    cf: CollectedFunction,
    profile: Option<&str>,
    weights: Option<&WeightConfig>,
    refactoring_threshold: f64,
) -> FunctionQualityReport {
    let metrics = MetricBreakdown {
        cognitive_flow: cf.cfc,
        data_complexity: cf.dci,
        identifier_reference: cf.irc,
        dependency_coupling: cf.dc,
        structural: cf.sm,
    };

    let (score, breakdown) = compute_score(&metrics, weights, profile);
    let grade = grade_from_score(score, profile);
    let needs_refactoring = score < refactoring_threshold;
    let flags = generate_flags(&metrics);

    FunctionQualityReport {
        name: cf.name,
        inferred_name: cf.inferred_name,
        score,
        grade,
        needs_refactoring,
        flags,
        metrics,
        score_breakdown: breakdown,
        location: SourceLocation {
            file: String::new(),
            start_line: cf.start_line,
            end_line: cf.end_line,
            start_col: 0,
            end_col: 0,
        },
        is_async: cf.is_async,
        is_generator: cf.is_generator,
    }
}

fn build_class_report(
    cc: CollectedClass,
    profile: Option<&str>,
    weights: Option<&WeightConfig>,
    refactoring_threshold: f64,
) -> ClassQualityReport {
    let method_reports: Vec<FunctionQualityReport> = cc
        .methods
        .into_iter()
        .map(|m| build_fn_report(m, profile, weights, refactoring_threshold))
        .collect();

    let method_scores: Vec<(f64, u32)> = method_reports
        .iter()
        .map(|r| (r.score, r.metrics.structural.loc.max(1)))
        .collect();

    let class_score = if method_scores.is_empty() {
        100.0
    } else {
        aggregate_scores(&method_scores)
    };

    let grade = grade_from_score(class_score, profile);
    let needs_refactoring = class_score < refactoring_threshold;
    let method_count = method_reports.len() as u32;
    let total_loc: u32 = method_reports.iter().map(|r| r.metrics.structural.loc).sum();
    let max_nesting = method_reports
        .iter()
        .map(|r| r.metrics.structural.max_nesting_depth)
        .max()
        .unwrap_or(0);

    let structural = StructuralResult {
        loc: total_loc,
        total_lines: 0,
        parameter_count: 0,
        max_nesting_depth: max_nesting,
        return_count: 0,
        method_count: Some(method_count),
        raw_score: compute_sm_raw(total_loc, 0, max_nesting, 0),
    };

    ClassQualityReport {
        name: cc.name,
        score: class_score,
        grade,
        needs_refactoring,
        flags: vec![],
        structural_metrics: structural,
        methods: method_reports,
        location: SourceLocation {
            file: String::new(),
            start_line: cc.start_line,
            end_line: cc.end_line,
            start_col: 0,
            end_col: 0,
        },
    }
}
