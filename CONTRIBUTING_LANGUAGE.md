# Adding a New Language to qualitas

This guide walks you through adding support for a new programming language. You only need to create **one file** and add **one line** to the registry.

## Overview

qualitas uses an event-based IR (intermediate representation). Your language adapter:
1. Parses source code using whatever parser is best for the language
2. Walks the AST to find functions, classes, and imports
3. For each function body, emits a `Vec<QualitasEvent>` describing the metric-relevant constructs

The 5 metric collectors (CFC, DCI, IRC, DC, SM) consume these events. You never touch the scoring logic.

## Step-by-step

### 1. Create `src/languages/<lang>.rs`

```rust
use crate::ir::events::*;
use crate::ir::language::*;

pub struct PythonAdapter;

impl LanguageAdapter for PythonAdapter {
    fn name(&self) -> &str { "Python" }

    fn extensions(&self) -> &[&str] { &[".py"] }

    fn extract(&self, source: &str, file_path: &str) -> Result<FileExtraction, String> {
        // 1. Parse source with your parser (e.g., tree-sitter-python)
        // 2. Walk AST to find functions/classes
        // 3. For each function body, emit QualitasEvents
        // 4. Return FileExtraction
        todo!()
    }

    fn threshold_overrides(&self) -> Option<ThresholdOverrides> {
        // Optional: adjust thresholds for your language
        Some(ThresholdOverrides {
            norm_sm_loc: Some(60.0),  // Python functions tend to be shorter
            ..Default::default()
        })
    }
}
```

### 2. Register in `src/languages/mod.rs`

```rust
pub mod python;  // Add module declaration

fn all_adapters() -> Vec<Box<dyn LanguageAdapter>> {
    vec![
        Box::new(typescript::TypeScriptAdapter),
        Box::new(python::PythonAdapter),  // Add this line
    ]
}
```

### 3. Add parser dependency to `Cargo.toml`

```toml
[dependencies]
tree-sitter = { version = "0.24", optional = true }
tree-sitter-python = { version = "0.23", optional = true }

[features]
lang-python = ["dep:tree-sitter", "dep:tree-sitter-python"]
```

### 4. Write tests

Add `#[cfg(test)]` tests in your adapter file that verify event emission for key language constructs.

## Event Mapping Reference

Your adapter needs to emit these events for the corresponding language constructs:

### Control Flow (consumed by CFC)

| Your language construct | Event to emit |
|------------------------|---------------|
| `if` / `elif` / `else if` | `ControlFlow(ControlFlowEvent { kind: If, ... })` |
| `for` / `for...in` | `ControlFlow(... kind: For / ForIn ...)` |
| `while` / `do while` | `ControlFlow(... kind: While / DoWhile ...)` |
| `switch` / `match` | `ControlFlow(... kind: Switch / PatternMatch ...)` |
| `catch` / `except` | `ControlFlow(... kind: Catch ...)` |
| `with` / context managers | `ControlFlow(... kind: ContextManager ...)` |
| `&&` / `and` | `LogicOp(LogicOpEvent::And)` |
| `\|\|` / `or` | `LogicOp(LogicOpEvent::Or)` |
| `??` / null coalescing | `LogicOp(LogicOpEvent::NullCoalesce)` |
| `? :` / ternary | `LogicOp(LogicOpEvent::Ternary)` |
| Recursive self-call | `RecursiveCall` |
| Labeled break/continue | `LabeledFlow` |
| `.then()` / `.catch()` | `AsyncComplexity(AsyncEvent::PromiseChain)` |
| `await` (nested) | `AsyncComplexity(AsyncEvent::Await)` |
| `go func()` / `spawn` | `AsyncComplexity(AsyncEvent::Spawn)` |
| Lambda as callback arg | `NestedCallback` |

### Nesting (consumed by CFC + SM)

After every `ControlFlow` event, emit `NestingEnter` before the branch body and `NestingExit` after:

```
ControlFlow(If)
NestingEnter        ← depth increases
  ... events inside if body ...
NestingExit         ← depth decreases
```

`NestingEnter`/`NestingExit` MUST be balanced.

### Nested Functions (consumed by SM, IRC)

When walking into a nested function/lambda body, wrap the body events:

```
NestedCallback      ← CFC penalty
NestedFunctionEnter ← SM/IRC stop counting here
NestingEnter
  ... events inside lambda body ...
NestingExit
NestedFunctionExit  ← SM/IRC resume
```

### Operators & Operands (consumed by DCI — Halstead metrics)

| Construct | Event |
|-----------|-------|
| `+`, `-`, `*`, `/`, `%`, etc. | `Operator(OperatorEvent { name: "+" })` |
| `=`, `+=`, `-=`, etc. | `Operator(...)` |
| `!`, `typeof`, `not`, etc. | `Operator(...)` |
| `++`, `--` | `Operator(...)` |
| Variable reference | `Operand(OperandEvent { name: "varName" })` |
| String literal | `Operand(OperandEvent { name: "hello" })` |
| Numeric literal | `Operand(OperandEvent { name: "42" })` |
| Boolean literal | `Operand(OperandEvent { name: "true" })` |
| Null/None | `Operand(OperandEvent { name: "null" })` |
| `this`/`self` | `Operand(OperandEvent { name: "this" })` |

### Identifiers (consumed by IRC)

| Construct | Event |
|-----------|-------|
| Variable declaration | `IdentDeclaration(IdentEvent { name, byte_offset })` |
| Variable reference | `IdentReference(IdentEvent { name, byte_offset })` |

### Dependencies (consumed by DC)

| Construct | Event |
|-----------|-------|
| `import` statement | Add to `FileExtraction.imports` |
| `importedObj.method()` call | `ApiCall(ApiCallEvent { object, method })` |

### Structural (consumed by SM)

| Construct | Event |
|-----------|-------|
| `return` / `yield` | `ReturnStatement` |

## What You Do NOT Touch

- `src/types.rs` — output types
- `src/scorer/` — scoring math
- `src/constants.rs` — global defaults
- `src/lib.rs` — napi exports
- `src/analyzer.rs` — orchestrator
- `src/metrics/` — metric collectors

## Using tree-sitter

tree-sitter is the recommended parser for non-JS/TS languages. It provides grammars for 100+ languages with a single Rust API.

```rust
use tree_sitter::{Parser, Node};

fn parse_python(source: &str) -> Result<tree_sitter::Tree, String> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|e| e.to_string())?;
    parser.parse(source, None)
        .ok_or("Failed to parse".to_string())
}
```

tree-sitter nodes have `.kind()` (e.g., `"if_statement"`, `"for_statement"`) and `.child()`/`.children()` for traversal. Use `.is_named()` to skip syntax tokens.

## Conformance Requirements

Every language adapter must satisfy:
- Non-empty events for every extracted function
- Balanced `NestingEnter`/`NestingExit` counts
- Balanced `NestedFunctionEnter`/`NestedFunctionExit` counts
- Correct function boundary extraction (name, byte span, param count)
