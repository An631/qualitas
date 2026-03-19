# Changelog

## 0.5.0

### Minor Changes

- [#41](https://github.com/An631/qualitas/pull/41) [`c330afb`](https://github.com/An631/qualitas/commit/c330afb2545a8f8515d73dfa4797d4576aba5f8e) Thanks [@An631](https://github.com/An631)! - Add Java language adapter via tree-sitter-java with full support for all Java control flow, anonymous inner classes, nested classes, lambdas, try-with-resources, and labeled break/continue

## 0.4.1

### Patch Changes

- [#38](https://github.com/An631/qualitas/pull/38) [`4356d79`](https://github.com/An631/qualitas/commit/4356d79ea0466c070fa1a3cd7844ffca7da10ce3) Thanks [@An631](https://github.com/An631)! - Fix Go adapter logical LOC counting to walk into statement_list nodes

- [#39](https://github.com/An631/qualitas/pull/39) [`e92a493`](https://github.com/An631/qualitas/commit/e92a493cf2b2b2c993cca551ff6728f1cccf4c96) Thanks [@An631](https://github.com/An631)! - Add pre-push hook to verify package-lock.json is in sync with package.json

## 0.4.0

### Minor Changes

- [#36](https://github.com/An631/qualitas/pull/36) [`e88ac3f`](https://github.com/An631/qualitas/commit/e88ac3feba70639759153d228aed03ae7b819ec4) Thanks [@An631](https://github.com/An631)! - Add Go language adapter via tree-sitter-go with full support for all Go control flow, goroutine detection, defer handling, and method receiver grouping

## Unreleased

### Minor Changes

- Added Java language adapter via `tree-sitter-java` (`.java` files)
  - Full support for all Java control flow: if/else, for, enhanced for, while, do-while, switch, try/catch/finally
  - First adapter to use `DoWhile` event kind
  - Try-with-resources emits `ContextManager` (like Python's `with`)
  - Ternary operator (`? :`) emits `LogicOp(Ternary)`
  - Lambda expressions emit `NestedCallback` + `NestedFunctionEnter/Exit`
  - Labeled break/continue emits `LabeledFlow`
  - Anonymous inner classes extracted as separate `ClassExtraction` entries; methods isolated via `NestedFunctionEnter/Exit`
  - Nested/inner classes extracted with `OuterClass.InnerClass` naming
  - Constructors named after their class
  - Imports tracked by last identifier segment for per-function coupling analysis
  - `instanceof` tracked as an operator
  - Method references (`::`) tracked as an operator

- Added Go language adapter via `tree-sitter-go` (`.go` files)
  - Full support for all Go control flow: if/else, for variants, switch, type switch, select
  - Goroutine detection (`go func()`) emits `AsyncComplexity(Spawn)`
  - `defer` bodies excluded from CFC but included in DCI
  - Method receivers grouped into `ClassExtraction` by type name
  - Higher CFC thresholds (18/25) to accommodate Go's `if err != nil` pattern
  - Imports tracked by package name or alias for per-function coupling analysis

## 0.3.1

### Patch Changes

- [#34](https://github.com/An631/qualitas/pull/34) [`fd27b3a`](https://github.com/An631/qualitas/commit/fd27b3ae67e3b5295301e89ad5607f959a0aada8) Thanks [@An631](https://github.com/An631)! - Fix config exclude list replacing defaults instead of merging with them

## 0.3.0

### Minor Changes

- [`8a59214`](https://github.com/An631/qualitas/commit/8a59214304ed113c7dd5ee9a9b62697c0023a885) - Removing unnecessary JS cli in favor or RS only CLI

## 0.2.7

### Patch Changes

- [`eef8668`](https://github.com/An631/qualitas/commit/eef8668d7e9ee488f49374f5386b3778cc03de78) - Syncing the package lock file

- [`fc14359`](https://github.com/An631/qualitas/commit/fc14359ad543e0756ae6e3d573a60fc53325b5de) - Updating readme to have latest features reflected.

## 0.2.6

### Patch Changes

- [`bffb57d`](https://github.com/An631/qualitas/commit/bffb57db4d89fd26d25f3390406f0993888d1f04) - Fixing the packagelock file syncing issues.

- [#30](https://github.com/An631/qualitas/pull/30) [`fb90be2`](https://github.com/An631/qualitas/commit/fb90be20c3bbd4be0455a90365c925c9db0a0b3c) Thanks [@An631](https://github.com/An631)! - Revert post-publish lockfile sync step (blocked by branch protection)

## 0.2.5

### Patch Changes

- [#28](https://github.com/An631/qualitas/pull/28) [`d02f84f`](https://github.com/An631/qualitas/commit/d02f84f7efc134fc4c488b68b491ee06aba94119) Thanks [@An631](https://github.com/An631)! - Sync package-lock.json after platform bindings are published to prevent lockfile drift

- [`1ab1735`](https://github.com/An631/qualitas/commit/1ab1735ba801f3ad057b60e1b64db99ddc62683e) - Adding qualitas.config.js file to default excludes.

## 0.2.4

### Patch Changes

- [`5435629`](https://github.com/An631/qualitas/commit/5435629882feef7ce08e508776ca5aca81395411) - Improving configuration loading flexibility for consumers and adding per language flag overrides logic.

## 0.2.3

### Patch Changes

- [`e48ce9e`](https://github.com/An631/qualitas/commit/e48ce9efb789e75549cc2396b6f67d22f1991384) - Supporting parallel file analysis execution for performance and adding perf tracking logs. Included a new parameter to provide a config file to the cli commands.

- [`2003d45`](https://github.com/An631/qualitas/commit/2003d45758b9ce7b43f2521794d9d522af9a9cd3) - Making linting and type rules stricter

## 0.2.2

### Patch Changes

- [`43649f4`](https://github.com/An631/qualitas/commit/43649f4e686164052e3abd53910b67112dd9439d) - Adding optionalDependencies version syncing logic

## 0.2.1

### Patch Changes

- [#23](https://github.com/An631/qualitas/pull/23) [`7ad10cc`](https://github.com/An631/qualitas/commit/7ad10ccea6844c960aec7ffe2e5941cda9e0c9b5) Thanks [@An631](https://github.com/An631)! - Refactor Python adapter to resolve quality flags and update documentation for Python support

## 0.2.0

### Minor Changes

- [#21](https://github.com/An631/qualitas/pull/21) [`68d6a3a`](https://github.com/An631/qualitas/commit/68d6a3aec2eb108efe845ac38596dbfa9abf371f) Thanks [@An631](https://github.com/An631)! - Adding support for python files

## 0.1.9

### Patch Changes

- [#19](https://github.com/An631/qualitas/pull/19) [`310885c`](https://github.com/An631/qualitas/commit/310885c2af8a6b194171a19da4c8499dbd602617) Thanks [@An631](https://github.com/An631)! - Fix panic on multi-byte UTF-8 string literals when truncating to 32 bytes

## 0.1.8

### Patch Changes

- [#17](https://github.com/An631/qualitas/pull/17) [`1b1b8b5`](https://github.com/An631/qualitas/commit/1b1b8b5ad24a6016c8d42c72fb58f19eee117a90) Thanks [@An631](https://github.com/An631)! - Improving release.yml binaries publishing errors

## 0.1.7

### Patch Changes

- [#15](https://github.com/An631/qualitas/pull/15) [`7c4d66e`](https://github.com/An631/qualitas/commit/7c4d66e9cbb381716d7dc8fb21dfa405f54108a3) Thanks [@An631](https://github.com/An631)! - fix: release workflow now builds binaries before publishing platform packages

## 0.1.6

### Patch Changes

- [#13](https://github.com/An631/qualitas/pull/13) [`1421ffe`](https://github.com/An631/qualitas/commit/1421ffecc452a65d751b29c32ff854acff1045d0) Thanks [@An631](https://github.com/An631)! - Merging the binaries publishing pipeline with teh release pipeline to ensure they happen at the same time as teh main qualitas package

## 0.1.5

### Patch Changes

- [#11](https://github.com/An631/qualitas/pull/11) [`aa2309c`](https://github.com/An631/qualitas/commit/aa2309ccce79b610ef9867b4742788d40ccecdfb) Thanks [@An631](https://github.com/An631)! - Adding version syncing for platform binaries packages

## 0.1.4

### Patch Changes

- [#9](https://github.com/An631/qualitas/pull/9) [`40fd1be`](https://github.com/An631/qualitas/commit/40fd1be8e03f6eba6bfc5212f81cab5905c5f455) Thanks [@An631](https://github.com/An631)! - Enabling publishing of platform binaries through github cli

## 0.1.3

### Patch Changes

- [#7](https://github.com/An631/qualitas/pull/7) [`40b4f3f`](https://github.com/An631/qualitas/commit/40b4f3fd46cebd4bfb3f168ab6c3b100f415c347) Thanks [@An631](https://github.com/An631)! - Adding package.json file formatting verification

## 0.1.2

### Patch Changes

- [#5](https://github.com/An631/qualitas/pull/5) [`2483254`](https://github.com/An631/qualitas/commit/24832544d3efd355d40874e7aea4276b9c358199) Thanks [@An631](https://github.com/An631)! - Adding a version script to ensure the version command is not misinterpreted by Shell

## 0.1.1

### Patch Changes

- [#1](https://github.com/An631/qualitas/pull/1) [`4ff4e8b`](https://github.com/An631/qualitas/commit/4ff4e8b1aa8bcfdbb5227b54e05caefe2df37119) Thanks [@An631](https://github.com/An631)! - Introducing the RELEASING documentation and using this as a test for first ever automatic NPM publish and git releases

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-03-05

### Added

- Five-pillar quality scoring: CFC, DCI, IRC, DC, SM with exponential saturation model
- TypeScript/JavaScript language support via oxc_parser
- Rust language support via syn
- Python language support via tree-sitter-python (`.py`, `.pyi`)
- Event-based IR architecture for language-agnostic metric collection
- Standalone Rust CLI binary (`qualitas`) with 7 output formats (text, compact, detail, flagged, json, markdown, summary)
- Node.js CLI via `npx qualitas`
- Programmatic TypeScript API: `analyzeSource`, `analyzeFile`, `analyzeProject`, `quickScore`
- Pre-built native binaries for 5 platforms (macOS arm64/x64, Linux x64/arm64, Windows x64)
- `qualitas.config.js` configuration file support
- Per-language flag threshold overrides
- `failOnFlags` option for zero-tolerance CI mode (`warn` or `error`)
- Weight profiles: default, cc-focused, data-focused, strict
- Match/switch arm CFC discount (0.25x per arm instead of 1x)
- Logical LOC (statement count) instead of physical LOC for fairer scoring
- IRC closure capture detection (parent-scope variable references count toward parent IRC)
- 103 Rust unit tests, 52 JavaScript integration tests
