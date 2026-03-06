# Changelog

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
