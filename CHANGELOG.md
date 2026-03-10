# Changelog

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
