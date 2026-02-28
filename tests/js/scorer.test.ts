/**
 * Integration tests for the composite scorer via the native binding.
 *
 * These tests use `analyzeSource` from the JS wrapper which calls into
 * the native Rust binding.
 */

// NOTE: These tests require the native binding to be built first (`npm run build`).
// Run: cargo build --release && napi build --platform --release
// Then: npm test

let analyzeSource: typeof import('../../js/index').analyzeSource;

beforeAll(async () => {
  try {
    const mod = await import('../../js/index.js');
    analyzeSource = mod.analyzeSource;
  } catch (err) {
    // Skip if native binding not available
    console.warn('Native binding not available, skipping integration tests:', (err as Error).message);
  }
});

function skipIfNoBinding(fn: () => void): () => void {
  return () => {
    if (!analyzeSource) {
      console.warn('Skipping: no native binding');
      return;
    }
    fn();
  };
}

// ─── Clean code ──────────────────────────────────────────────────────────────

describe('analyzeSource — clean code', () => {
  it('returns score >= 80 for trivial functions', skipIfNoBinding(() => {
    const src = `
export function add(a: number, b: number): number {
  return a + b;
}
`;
    const report = analyzeSource(src, 'add.ts');
    expect(report.score).toBeGreaterThanOrEqual(80);
    expect(report.grade).toBe('A');
    expect(report.needsRefactoring).toBe(false);
  }));

  it('returns no flags for simple utility', skipIfNoBinding(() => {
    const src = `
export function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}
`;
    const report = analyzeSource(src, 'capitalize.ts');
    const fnReport = report.functions[0];
    expect(fnReport).toBeDefined();
    expect(fnReport!.score).toBeGreaterThan(60);
  }));
});

// ─── Complex code ─────────────────────────────────────────────────────────────

describe('analyzeSource — complex code', () => {
  it('returns low score for deeply nested function', skipIfNoBinding(() => {
    const src = `
function processOrders(orders: any[], config: any, logger: any, db: any, cache: any, validator: any) {
  const results: any[] = [];
  for (const order of orders) {
    if (order.status === 'pending') {
      if (order.items && order.items.length > 0) {
        for (const item of order.items) {
          if (item.quantity > 0) {
            if (item.price !== undefined) {
              try {
                if (validator.isValid(item)) {
                  if (cache.has(item.id)) {
                    cache.invalidate(item.id);
                  } else {
                    db.insert(item);
                  }
                  results.push({ status: 'processed' });
                }
              } catch (err: any) {
                logger.error(err.message);
              }
            }
          }
        }
      }
    }
  }
  return results;
}
`;
    const report = analyzeSource(src, 'nested.ts');
    expect(report.score).toBeLessThan(65);
    expect(report.needsRefactoring).toBe(true);

    const fn = report.functions[0];
    expect(fn).toBeDefined();
    const cfcFlag = fn!.flags.find(f => f.flagType === 'HIGH_COGNITIVE_FLOW');
    expect(cfcFlag).toBeDefined();
  }));

  it('flags too many params', skipIfNoBinding(() => {
    const src = `function f(a: any, b: any, c: any, d: any, e: any, f: any) { return a; }`;
    const report = analyzeSource(src, 'params.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    const paramsFlag = fn!.flags.find(f => f.flagType === 'TOO_MANY_PARAMS');
    expect(paramsFlag).toBeDefined();
    expect(paramsFlag!.severity).toBe('error');
  }));

  it('flags TOO_LONG for functions over 40 lines', skipIfNoBinding(() => {
    const body = Array.from({ length: 45 }, (_, i) => `  const v${i} = ${i} + 1;`).join('\n');
    const src = `function longFn() {\n${body}\n  return v0;\n}`;
    const report = analyzeSource(src, 'long.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    const locFlag = fn!.flags.find(f => f.flagType === 'TOO_LONG');
    expect(locFlag).toBeDefined();
    expect(fn!.metrics.structural.loc).toBeGreaterThanOrEqual(41);
  }));

  it('flags DEEP_NESTING for > 4 levels', skipIfNoBinding(() => {
    const src = `
function f(a: any) {
  if (a) {
    for (const x of a) {
      if (x) {
        while (x.active) {
          if (x.value > 0) {
            return x.value;
          }
        }
      }
    }
  }
}
`;
    const report = analyzeSource(src, 'nesting.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    expect(fn!.metrics.structural.maxNestingDepth).toBeGreaterThanOrEqual(4);
    const nestFlag = fn!.flags.find(f => f.flagType === 'DEEP_NESTING');
    expect(nestFlag).toBeDefined();
  }));
});

// ─── SourceLocation correctness ───────────────────────────────────────────────

describe('analyzeSource — SourceLocation line numbers', () => {
  it('reports 1-based line numbers (not byte offsets)', skipIfNoBinding(() => {
    const src = [
      '// line 1',
      '// line 2',
      '// line 3',
      'function foo(x: number): number {',
      '  return x * 2;',
      '}',
    ].join('\n');
    const report = analyzeSource(src, 'loc.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    // startLine must be a small line number, never a byte offset (which would be >> 100)
    expect(fn!.location.startLine).toBe(4);
    expect(fn!.location.endLine).toBe(6);
    expect(fn!.location.startLine).toBeLessThanOrEqual(fn!.location.endLine);
  }));

  it('startLine of second function is after first function', skipIfNoBinding(() => {
    const src = `
function first() { return 1; }

function second() { return 2; }
`;
    const report = analyzeSource(src, 'two_fns.ts');
    expect(report.functions).toHaveLength(2);
    const [f1, f2] = report.functions;
    expect(f1.location.startLine).toBeLessThan(f2.location.startLine);
  }));
});

// ─── Dependency Coupling (DC) ─────────────────────────────────────────────────

describe('analyzeSource — dependency coupling', () => {
  it('detects distinct API calls from imported module bindings', skipIfNoBinding(() => {
    const src = `
import fs from 'fs';
import path from 'path';

function readConfig(dir: string) {
  const resolved = path.resolve(dir, 'config.json');
  const content = fs.readFileSync(resolved, 'utf8');
  return JSON.parse(content);
}
`;
    const report = analyzeSource(src, 'config.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    // path.resolve + fs.readFileSync = 2 distinct API calls
    expect(fn!.metrics.dependencyCoupling.distinctApiCalls).toBeGreaterThanOrEqual(2);
  }));

  it('reports file-level import count and external packages', skipIfNoBinding(() => {
    const src = `
import axios from 'axios';
import fs from 'fs';
import path from 'path';

export function fetch() { return axios.get('/data'); }
`;
    const report = analyzeSource(src, 'imports.ts');
    expect(report.fileDependencies.importCount).toBe(3);
    expect(report.fileDependencies.externalRatio).toBeGreaterThan(0);
    expect(report.fileDependencies.externalPackages).toContain('axios');
  }));

  it('reports zero DC for functions with no import API calls', skipIfNoBinding(() => {
    const src = `function pure(a: number, b: number) { return a + b; }`;
    const report = analyzeSource(src, 'pure.ts');
    const fn = report.functions[0];
    expect(fn!.metrics.dependencyCoupling.distinctApiCalls).toBe(0);
    expect(fn!.metrics.dependencyCoupling.rawScore).toBe(0);
  }));
});

// ─── Identifier Reference Complexity (IRC) ───────────────────────────────────

describe('analyzeSource — identifier reference complexity', () => {
  it('computes non-zero IRC for variables used across many lines', skipIfNoBinding(() => {
    const src = `
export function generateReport(records: any[]): string {
  const sections: string[] = [];
  const summary: Record<string, number> = {};
  const errors: string[] = [];

  for (const record of records) {
    if (record.type === 'sale') {
      const key = record.category ?? 'unknown';
      summary[key] = (summary[key] ?? 0) + record.amount;
      sections.push('Sale: ' + record.id);
    } else if (record.type === 'refund') {
      errors.push('Refund: ' + record.id);
      summary[record.category] = (summary[record.category] ?? 0) - record.amount;
    } else {
      errors.push('Unknown: ' + record.type);
    }
  }

  const header = 'Report (' + sections.length + ' items, ' + errors.length + ' errors)';
  const body = sections.join('\\n');
  const errorSection = errors.join('\\n');
  return header + '\\n' + body + '\\n' + errorSection;
}
`;
    const report = analyzeSource(src, 'report.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    expect(fn!.metrics.identifierReference.totalIrc).toBeGreaterThan(20);
    expect(fn!.metrics.identifierReference.hotspots.length).toBeGreaterThan(0);
  }));

  it('flags HIGH_IDENTIFIER_CHURN when IRC exceeds error threshold', skipIfNoBinding(() => {
    const lines = [
      'function buildString(items: string[]): string {',
      '  const result: string[] = [];',
      '  const prefix = "item-";',
      '  const separator = ", ";',
      ...Array.from({ length: 30 }, (_, i) =>
        `  if (items[${i}]) { result.push(prefix + items[${i}] + separator); }`
      ),
      '  return result.join(separator);',
      '}',
    ].join('\n');
    const report = analyzeSource(lines, 'irc.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    const ircFlag = fn!.flags.find(f => f.flagType === 'HIGH_IDENTIFIER_CHURN');
    expect(ircFlag).toBeDefined();
  }));
});

// ─── Arrow functions ──────────────────────────────────────────────────────────

describe('analyzeSource — arrow functions', () => {
  it('collects and analyzes const arrow functions', skipIfNoBinding(() => {
    const src = `
const add = (a: number, b: number): number => a + b;

const processItems = (items: any[]): any[] => {
  const result: any[] = [];
  for (const item of items) {
    if (item.active) {
      result.push(item);
    }
  }
  return result;
};
`;
    const report = analyzeSource(src, 'arrows.ts');
    expect(report.functions.length).toBeGreaterThanOrEqual(2);

    const addFn = report.functions.find(f => f.name === 'add');
    expect(addFn).toBeDefined();
    expect(addFn!.score).toBeGreaterThanOrEqual(80);

    const processFn = report.functions.find(f => f.name === 'processItems');
    expect(processFn).toBeDefined();
    expect(processFn!.metrics.structural.loc).toBeGreaterThan(0);
  }));

  it('reports isAsync=true for async functions', skipIfNoBinding(() => {
    const src = `async function fetchData(url: string) { return fetch(url); }`;
    const report = analyzeSource(src, 'async.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    expect(fn!.isAsync).toBe(true);
  }));
});

// ─── Class analysis ───────────────────────────────────────────────────────────

describe('analyzeSource — class analysis', () => {
  it('collects class methods and aggregates class score', skipIfNoBinding(() => {
    const src = `
class Calculator {
  add(a: number, b: number): number { return a + b; }
  subtract(a: number, b: number): number { return a - b; }
  multiply(a: number, b: number): number { return a * b; }
}
`;
    const report = analyzeSource(src, 'calculator.ts');
    expect(report.classes).toHaveLength(1);
    const cls = report.classes[0];
    expect(cls.name).toBe('Calculator');
    expect(cls.methods).toHaveLength(3);
    expect(cls.score).toBeGreaterThanOrEqual(80);
    expect(cls.grade).toBeDefined();
    // Class location must be valid line numbers
    expect(cls.location.startLine).toBeGreaterThanOrEqual(1);
    expect(cls.location.endLine).toBeGreaterThan(cls.location.startLine);
  }));

  it('class with complex methods scores lower than class with simple methods', skipIfNoBinding(() => {
    const simpleClass = `
class A {
  add(a: number, b: number) { return a + b; }
}
`;
    const complexClass = `
class B {
  process(a: any, b: any, c: any, d: any, e: any) {
    for (let i = 0; i < a; i++) {
      if (b && c) {
        for (let j = 0; j < d; j++) {
          if (e || !b) { return i + j; }
        }
      }
    }
  }
}
`;
    const simpleReport = analyzeSource(simpleClass, 'a.ts');
    const complexReport = analyzeSource(complexClass, 'b.ts');
    expect(simpleReport.classes[0].score).toBeGreaterThan(complexReport.classes[0].score);
  }));
});

// ─── Scoring invariants ───────────────────────────────────────────────────────

describe('analyzeSource — scoring invariants', () => {
  it('clean code scores higher than complex code', skipIfNoBinding(() => {
    const clean = `function add(a: number, b: number) { return a + b; }`;
    const complex = `
function f(a: any, b: any, c: any, d: any, e: any) {
  for (let i = 0; i < a; i++) {
    if (b && c) {
      for (let j = 0; j < d; j++) {
        if (e || !b) {
          try {
            if (a > 0 && b < 10) { return i + j; }
          } catch(e) { return -1; }
        }
      }
    }
  }
}`;
    const cleanReport = analyzeSource(clean, 'clean.ts');
    const complexReport = analyzeSource(complex, 'complex.ts');
    expect(cleanReport.score).toBeGreaterThan(complexReport.score);
  }));

  it('score is always between 0 and 100', skipIfNoBinding(() => {
    const extremes = [
      'function f() {}',
      `function f(a:any,b:any,c:any,d:any,e:any,f:any,g:any){for(;;){if(a&&b&&c){for(;;){if(d||e){try{if(f!==g){return 1;}}catch(e){}}}}}}`,
    ];
    for (const src of extremes) {
      const report = analyzeSource(src, 'x.ts');
      expect(report.score).toBeGreaterThanOrEqual(0);
      expect(report.score).toBeLessThanOrEqual(100);
    }
  }));

  it('empty file returns score 100', skipIfNoBinding(() => {
    const report = analyzeSource('', 'empty.ts');
    expect(report.score).toBe(100);
    expect(report.functions).toHaveLength(0);
  }));

  it('ScoreBreakdown penalties sum to total penalty', skipIfNoBinding(() => {
    const src = `
function messy(a: any, b: any, c: any, d: any) {
  for (let i = 0; i < a; i++) {
    if (b) {
      for (let j = 0; j < c; j++) {
        if (d) { return i + j; }
      }
    }
  }
}
`;
    const report = analyzeSource(src, 'messy.ts');
    const fn = report.functions[0];
    expect(fn).toBeDefined();
    const bd = fn!.scoreBreakdown;
    const sumPenalties = bd.cfcPenalty + bd.dciPenalty + bd.ircPenalty + bd.dcPenalty + bd.smPenalty;
    expect(Math.abs(sumPenalties - bd.totalPenalty)).toBeLessThan(0.01);
    expect(fn!.score).toBeCloseTo(100 - bd.totalPenalty, 1);
  }));
});
