# Changelog

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
