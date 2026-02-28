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
});

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
});
