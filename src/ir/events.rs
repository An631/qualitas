//! Language-agnostic IR events emitted by language adapters.
//!
//! Each event represents a single metric-relevant observation from an AST walk.
//! The 5 metric collectors consume subsets of these events to compute their scores.
//!
//! # Event ordering contract
//!
//! Language adapters MUST emit events in AST walk order:
//! - `NestingEnter` before events inside a block
//! - `NestingExit` after events inside a block
//! - `NestingEnter`/`NestingExit` must be balanced
//! - `ControlFlow` is emitted at the point where the branch is encountered
//!   (before `NestingEnter` for the branch body)

/// A single event emitted by a language adapter while walking a function body.
#[derive(Debug, Clone)]
pub enum QualitasEvent {
    // ── Control flow (consumed by CFC) ──────────────────────────────────
    /// A control-flow branch point: if, for, while, switch, catch, etc.
    ControlFlow(ControlFlowEvent),

    /// A logical or ternary operator in an expression context.
    LogicOp(LogicOpEvent),

    /// A recursive call (function calls itself by name).
    RecursiveCall,

    /// A labeled statement, or a labeled break/continue.
    LabeledFlow,

    /// Async-specific complexity: promise chains, goroutine spawns, etc.
    AsyncComplexity(AsyncEvent),

    /// An arrow function / lambda / closure used as a callback argument.
    NestedCallback,

    // ── Nesting depth (consumed by CFC + SM) ────────────────────────────
    /// Enter a nesting level (block, branch body, loop body, closure body).
    NestingEnter,

    /// Exit a nesting level.
    NestingExit,

    // ── Halstead operators & operands (consumed by DCI) ─────────────────
    /// An operator occurrence (+, &&, =, typeof, etc.)
    Operator(OperatorEvent),

    /// An operand occurrence (identifier, literal, this, null, etc.)
    Operand(OperandEvent),

    // ── Identifier tracking (consumed by IRC) ───────────────────────────
    /// An identifier is declared/bound at this byte offset.
    IdentDeclaration(IdentEvent),

    /// An identifier is referenced at this byte offset.
    IdentReference(IdentEvent),

    // ── Dependency coupling (consumed by DC) ────────────────────────────
    /// A module-qualified API call: `object.method()`.
    ApiCall(ApiCallEvent),

    // ── Nested function boundary (consumed by SM, IRC) ─────────────────
    /// Entering a nested function/arrow/lambda body.
    /// SM and IRC stop counting at this boundary; CFC and DCI continue.
    /// Must be paired with `NestedFunctionExit`.
    NestedFunctionEnter,

    /// Exiting a nested function/arrow/lambda body.
    NestedFunctionExit,

    // ── Structural (consumed by SM) ─────────────────────────────────────
    /// A return/yield/throw statement.
    ReturnStatement,
}

// ─── Event payload types ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ControlFlowEvent {
    pub kind: ControlFlowKind,
    /// Does this node have an else/alternate branch?
    pub has_else: bool,
    /// Is the else branch another if (else-if chain)?
    pub else_is_if: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFlowKind {
    If,
    For,
    ForIn,
    ForOf,
    While,
    DoWhile,
    Switch,
    Catch,
    /// Python `with`, Go `select`, Rust `match` arm, etc.
    ContextManager,
    /// Pattern matching (Rust match, Python match-case, etc.)
    PatternMatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicOpEvent {
    /// `&&` or `and`
    And,
    /// `||` or `or`
    Or,
    /// `??` or equivalent null-coalescing
    NullCoalesce,
    /// `? :` or inline conditional
    Ternary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncEvent {
    /// `.then()` / `.catch()` promise chains
    PromiseChain,
    /// `await` in a nested scope
    Await,
    /// `go func()`, `asyncio.create_task()`, thread spawn, etc.
    Spawn,
}

#[derive(Debug, Clone)]
pub struct OperatorEvent {
    /// The operator symbol: "+", "&&", "=", "typeof", "as", etc.
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct OperandEvent {
    /// The operand text: variable name, literal value, "null", "this", etc.
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct IdentEvent {
    pub name: String,
    pub byte_offset: u32,
}

#[derive(Debug, Clone)]
pub struct ApiCallEvent {
    /// The object/module name, e.g., "fs" in `fs.readFile()`
    pub object: String,
    /// The method name, e.g., "readFile"
    pub method: String,
}
