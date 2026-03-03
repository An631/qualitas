/**
 * Language-agnostic scoring tests.
 *
 * These tests verify scoring invariants, quickScore consistency, and reporter
 * output formatting — none depend on any specific language syntax.
 * They use minimal TypeScript source strings as a convenient vehicle, but the
 * properties tested hold for any language adapter.
 */

let analyzeSource: typeof import('../../js/index').analyzeSource;
let quickScore: typeof import('../../js/index').quickScore;

beforeAll(async () => {
  try {
    const mod = await import('../../js/index.js');
    analyzeSource = mod.analyzeSource;
    quickScore = mod.quickScore;
  } catch (err) {
    console.warn(
      'Native binding not available, skipping integration tests:',
      (err as Error).message,
    );
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

// ─── Scoring invariants ───────────────────────────────────────────────────────

describe('scoring invariants', () => {
  it(
    'score is always between 0 and 100',
    skipIfNoBinding(() => {
      const extremes = [
        'function f() {}',
        `function f(a:any,b:any,c:any,d:any,e:any,f:any,g:any){for(;;){if(a&&b&&c){for(;;){if(d||e){try{if(f!==g){return 1;}}catch(e){}}}}}}`,
      ];
      for (const src of extremes) {
        const report = analyzeSource(src, 'x.ts');
        expect(report.score).toBeGreaterThanOrEqual(0);
        expect(report.score).toBeLessThanOrEqual(100);
      }
    }),
  );

  it(
    'empty file returns score 100',
    skipIfNoBinding(() => {
      const report = analyzeSource('', 'empty.ts');
      expect(report.score).toBe(100);
      expect(report.functions).toHaveLength(0);
    }),
  );

  it(
    'clean code scores higher than complex code',
    skipIfNoBinding(() => {
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
    }),
  );

  it(
    'ScoreBreakdown penalties sum to total penalty',
    skipIfNoBinding(() => {
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
      const sumPenalties =
        bd.cfcPenalty + bd.dciPenalty + bd.ircPenalty + bd.dcPenalty + bd.smPenalty;
      expect(Math.abs(sumPenalties - bd.totalPenalty)).toBeLessThan(0.01);
      expect(fn!.score).toBeCloseTo(100 - bd.totalPenalty, 1);
    }),
  );
});

// ─── quickScore ───────────────────────────────────────────────────────────────

describe('quickScore', () => {
  it(
    'returns compact shape with correct fields',
    skipIfNoBinding(() => {
      const src = `export function add(a: number, b: number) { return a + b; }`;
      const result = quickScore(src, 'add.ts');
      expect(typeof result.score).toBe('number');
      expect(['A', 'B', 'C', 'D', 'F']).toContain(result.grade);
      expect(typeof result.needsRefactoring).toBe('boolean');
      expect(typeof result.functionCount).toBe('number');
      expect(typeof result.flaggedFunctionCount).toBe('number');
      expect(Array.isArray(result.topFlags)).toBe(true);
    }),
  );

  it(
    'score matches analyzeSource for same input',
    skipIfNoBinding(() => {
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
    }),
  );

  it(
    'returns topFlags for code with high complexity indicators',
    skipIfNoBinding(() => {
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
      expect(result.score).toBeLessThan(90);
      expect(result.topFlags.length).toBeGreaterThan(0);
    }),
  );

  it(
    'clean code returns score >= 80 and empty topFlags',
    skipIfNoBinding(() => {
      const src = `export function clamp(v: number, min: number, max: number) { return Math.min(Math.max(v, min), max); }`;
      const result = quickScore(src, 'clamp.ts');
      expect(result.score).toBeGreaterThanOrEqual(80);
      expect(result.needsRefactoring).toBe(false);
      expect(result.topFlags).toHaveLength(0);
    }),
  );
});

// ─── scope filtering (text reporter) ─────────────────────────────────────────

describe('renderFileReport — scope filtering', () => {
  let renderFileReport: typeof import('../../js/reporters/text').renderFileReport;

  beforeAll(async () => {
    const mod = await import('../../js/reporters/text.js');
    renderFileReport = mod.renderFileReport;
  });

  it(
    'scope=function (default) includes function names',
    skipIfNoBinding(() => {
      const src = `
export function alpha(x: number) { return x; }
export function beta(x: number) { return x * 2; }
`;
      const report = analyzeSource(src, 'scope.ts');
      const output = renderFileReport(report, { scope: 'function' });
      expect(output).toContain('alpha');
      expect(output).toContain('beta');
    }),
  );

  it(
    'scope=file omits function names but shows file score',
    skipIfNoBinding(() => {
      const src = `
export function alpha(x: number) { return x; }
export function beta(x: number) { return x * 2; }
`;
      const report = analyzeSource(src, 'scope.ts');
      const output = renderFileReport(report, { scope: 'file' });
      expect(output).not.toMatch(/✓.*alpha/);
      expect(output).not.toMatch(/✓.*beta/);
      expect(output).toContain('scope.ts');
      expect(output).toMatch(/[ABCDF] —/);
    }),
  );

  it(
    'scope=class shows class summary, skips standalone functions',
    skipIfNoBinding(() => {
      const src = `
export function standalone(x: number) { return x; }
class MyService {
  compute(x: number) { return x * 2; }
}
`;
      const report = analyzeSource(src, 'cls.ts');
      const output = renderFileReport(report, { scope: 'class' });
      expect(output).toContain('MyService');
      expect(output).not.toMatch(/✓.*standalone|✗.*standalone/);
    }),
  );

  it(
    'default scope (no option) behaves like scope=function',
    skipIfNoBinding(() => {
      const src = `export function gamma(x: number) { return x + 1; }`;
      const report = analyzeSource(src, 'default.ts');
      const withExplicit = renderFileReport(report, { scope: 'function' });
      const withDefault = renderFileReport(report, {});
      expect(withDefault).toBe(withExplicit);
    }),
  );
});
