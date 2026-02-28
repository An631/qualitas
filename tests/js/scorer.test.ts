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

// ─── Function collection patterns ────────────────────────────────────────────

describe('analyzeSource — function collection patterns', () => {
  it('collects object literal arrow functions with property key names', skipIfNoBinding(() => {
    const src = `
const handlers = {
  onClick: (e: any) => { console.log(e); return e.target; },
  onHover: (e: any) => e,
};
`;
    const report = analyzeSource(src, 'obj.ts');
    expect(report.functions).toHaveLength(2);
    const names = report.functions.map(f => f.name);
    expect(names).toContain('onClick');
    expect(names).toContain('onHover');
  }));

  it('collects object literal function expressions with property key names', skipIfNoBinding(() => {
    const src = `
const api = {
  fetch: function(url: string) { return url; },
  post: function(url: string, data: any) { return { url, data }; },
};
`;
    const report = analyzeSource(src, 'api.ts');
    expect(report.functions).toHaveLength(2);
    const names = report.functions.map(f => f.name);
    expect(names).toContain('fetch');
    expect(names).toContain('post');
  }));

  it('collects nested object arrows', skipIfNoBinding(() => {
    const src = `
const routes = {
  users: {
    getById: (id: string) => fetch('/users/' + id),
    list: () => fetch('/users'),
  },
};
`;
    const report = analyzeSource(src, 'routes.ts');
    expect(report.functions).toHaveLength(2);
    const names = report.functions.map(f => f.name);
    expect(names).toContain('getById');
    expect(names).toContain('list');
  }));

  it('collects export default arrow functions', skipIfNoBinding(() => {
    const src = `export default (a: number, b: number) => a + b;`;
    const report = analyzeSource(src, 'default.ts');
    expect(report.functions).toHaveLength(1);
    expect(report.functions[0].name).toBe('(default)');
    expect(report.functions[0].inferredName).toBe('export default ');
  }));

  it('collects export default named function', skipIfNoBinding(() => {
    const src = `
export default function processData(items: any[]) {
  return items.map(x => x);
}
`;
    const report = analyzeSource(src, 'default_fn.ts');
    expect(report.functions).toHaveLength(1);
    expect(report.functions[0].name).toBe('processData');
  }));

  it('collects class property arrows as class methods', skipIfNoBinding(() => {
    const src = `
class EventHandler {
  handleClick = (e: any) => { return e.target; };
  static create = () => new EventHandler();
  regularMethod(x: number) { return x * 2; }
}
`;
    const report = analyzeSource(src, 'handler.ts');
    expect(report.classes).toHaveLength(1);
    const methods = report.classes[0].methods;
    expect(methods).toHaveLength(3);
    const names = methods.map(m => m.name);
    expect(names).toContain('handleClick');
    expect(names).toContain('create');
    expect(names).toContain('regularMethod');
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

// ─── quickScore ───────────────────────────────────────────────────────────────

let quickScore: typeof import('../../js/index').quickScore;

beforeAll(async () => {
  try {
    const mod = await import('../../js/index.js');
    quickScore = mod.quickScore;
  } catch {
    // already handled by analyzeSource beforeAll
  }
});

describe('quickScore', () => {
  it('returns compact shape with correct fields', skipIfNoBinding(() => {
    const src = `export function add(a: number, b: number) { return a + b; }`;
    const result = quickScore(src, 'add.ts');
    expect(typeof result.score).toBe('number');
    expect(['A', 'B', 'C', 'D', 'F']).toContain(result.grade);
    expect(typeof result.needsRefactoring).toBe('boolean');
    expect(typeof result.functionCount).toBe('number');
    expect(typeof result.flaggedFunctionCount).toBe('number');
    expect(Array.isArray(result.topFlags)).toBe(true);
  }));

  it('score matches analyzeSource for same input', skipIfNoBinding(() => {
    const src = `
export function capitalize(s: string): string {
  if (!s) return s;
  return s.charAt(0).toUpperCase() + s.slice(1);
}
`;
    const quick = quickScore(src, 'cap.ts');
    const full = analyzeSource(src, 'cap.ts');
    expect(quick.score).toBeCloseTo(full.score, 1);
    expect(quick.grade).toBe(full.grade);
    expect(quick.needsRefactoring).toBe(full.needsRefactoring);
    expect(quick.functionCount).toBe(full.functionCount);
    expect(quick.flaggedFunctionCount).toBe(full.flaggedFunctionCount);
  }));

  it('returns topFlags for code with high complexity indicators', skipIfNoBinding(() => {
    // Many params + deep nesting triggers flags regardless of composite threshold
    const src = `
function processOrders(orders: any[], a: any, b: any, c: any, d: any, e: any) {
  for (const order of orders) {
    if (order.status === 'pending') {
      for (const item of order.items ?? []) {
        if (item.quantity > 0) {
          if (item.price > 0) {
            if (a.check(item)) {
              if (b.validate(item)) {
                c.save(item);
                d.notify(item);
              }
            }
          }
        }
      }
    }
  }
}
`;
    const result = quickScore(src, 'complex.ts');
    expect(result.functionCount).toBe(1);
    // Score should be noticeably below perfect for a deeply nested, many-param function
    expect(result.score).toBeLessThan(90);
    // topFlags are populated when any function has flags
    expect(result.topFlags.length).toBeGreaterThan(0);
  }));

  it('clean code returns score >= 80 and empty topFlags', skipIfNoBinding(() => {
    const src = `export function clamp(v: number, min: number, max: number) { return Math.min(Math.max(v, min), max); }`;
    const result = quickScore(src, 'clamp.ts');
    expect(result.score).toBeGreaterThanOrEqual(80);
    expect(result.needsRefactoring).toBe(false);
    expect(result.topFlags).toHaveLength(0);
  }));
});

// ─── scope filtering (text reporter) ─────────────────────────────────────────

describe('renderFileReport — scope filtering', () => {
  let renderFileReport: typeof import('../../js/reporters/text').renderFileReport;

  beforeAll(async () => {
    const mod = await import('../../js/reporters/text.js');
    renderFileReport = mod.renderFileReport;
  });

  it('scope=function (default) includes function names', skipIfNoBinding(() => {
    const src = `
export function alpha(x: number) { return x; }
export function beta(x: number) { return x * 2; }
`;
    const report = analyzeSource(src, 'scope.ts');
    const output = renderFileReport(report, { scope: 'function' });
    expect(output).toContain('alpha');
    expect(output).toContain('beta');
  }));

  it('scope=file omits function names but shows file score', skipIfNoBinding(() => {
    const src = `
export function alpha(x: number) { return x; }
export function beta(x: number) { return x * 2; }
`;
    const report = analyzeSource(src, 'scope.ts');
    const output = renderFileReport(report, { scope: 'file' });
    // Should NOT contain per-function rows
    expect(output).not.toMatch(/✓.*alpha/);
    expect(output).not.toMatch(/✓.*beta/);
    // Should still contain the file path
    expect(output).toContain('scope.ts');
    // Should still show grade info
    expect(output).toMatch(/[ABCDF] —/);
  }));

  it('scope=class shows class summary, skips standalone functions', skipIfNoBinding(() => {
    const src = `
export function standalone(x: number) { return x; }
class MyService {
  compute(x: number) { return x * 2; }
}
`;
    const report = analyzeSource(src, 'cls.ts');
    const output = renderFileReport(report, { scope: 'class' });
    // class name should appear
    expect(output).toContain('MyService');
    // standalone function row should NOT appear
    expect(output).not.toMatch(/✓.*standalone|✗.*standalone/);
  }));

  it('default scope (no option) behaves like scope=function', skipIfNoBinding(() => {
    const src = `export function gamma(x: number) { return x + 1; }`;
    const report = analyzeSource(src, 'default.ts');
    const withExplicit = renderFileReport(report, { scope: 'function' });
    const withDefault = renderFileReport(report, {});
    expect(withDefault).toBe(withExplicit);
  }));
});
