/**
 * <Language> adapter integration tests.
 *
 * Copy this file to tests/<language>/adapter.test.ts and fill in the
 * source strings with your language's syntax.
 *
 * See tests/typescript/adapter.test.ts for a complete example.
 */

import { analyzeSource } from '../../js/index.js';

// Replace <Language> and <ext> with your language name and file extension.
// Replace source strings with equivalent code in your language.

// ─── Clean code ──────────────────────────────────────────────────────────────

describe('<Language> — clean code', () => {
  it('returns score >= 80 for trivial functions', () => {
    const src = `
// TODO: Write a simple function in your language
// Example: def add(a, b): return a + b
`;
    const report = analyzeSource(src, 'clean.<ext>');
    expect(report.score).toBeGreaterThanOrEqual(80);
    expect(report.grade).toBe('A');
  });
});

// ─── Complex code ─────────────────────────────────────────────────────────────

describe('<Language> — complex code', () => {
  it('returns low score for deeply nested function', () => {
    const src = `
// TODO: Write a deeply nested function (5+ levels) in your language
`;
    const report = analyzeSource(src, 'nested.<ext>');
    expect(report.score).toBeLessThan(65);
    expect(report.needsRefactoring).toBe(true);
  });

  it('flags too many params', () => {
    const src = `
// TODO: Write a function with 8+ parameters in your language
`;
    const report = analyzeSource(src, 'params.<ext>');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    const paramsFlag = fn!.flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(paramsFlag).toBeDefined();
  });
});

// ─── SourceLocation line numbers ──────────────────────────────────────────────

describe('<Language> — SourceLocation line numbers', () => {
  it('reports 1-based line numbers', () => {
    const src = `
// TODO: Write a function that starts on a known line number
`;
    const report = analyzeSource(src, 'loc.<ext>');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    expect(fn!.location.startLine).toBeGreaterThan(0);
    expect(fn!.location.startLine).toBeLessThanOrEqual(fn!.location.endLine);
  });
});

// ─── Function collection ──────────────────────────────────────────────────────

describe('<Language> — function collection', () => {
  it('collects all top-level functions', () => {
    const src = `
// TODO: Write 2+ functions in your language
`;
    const report = analyzeSource(src, 'fns.<ext>');
    expect(report.functions.length).toBeGreaterThanOrEqual(2);
  });
});
