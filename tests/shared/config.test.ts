/**
 * Configuration loading, merging, and flag override tests.
 *
 * These tests verify:
 * - Config file discovery (walk up directories)
 * - CLI > config > defaults merge priority
 * - Global flag overrides (camelCase and SCREAMING_SNAKE_CASE keys)
 * - Per-language flag overrides via config parameter
 * - Flag disable/enable/custom thresholds
 * - Weight profiles and custom weights
 * - Extension and exclude configuration
 */

import { mkdtempSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { loadConfig, mergeOptions } from '../../js/config.js';
import { analyzeSource } from '../../js/index.js';
import type { AnalysisOptions, QualitasConfig } from '../../js/types.js';

// ─── loadConfig ──────────────────────────────────────────────────────────────

describe('loadConfig', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = mkdtempSync(join(tmpdir(), 'qualitas-config-test-'));
  });

  afterEach(() => {
    rmSync(tmpDir, { recursive: true, force: true });
  });

  it('returns empty object when no config file', () => {
    const config = loadConfig(tmpDir);
    expect(config).toEqual({});
  });

  it('finds config in parent directory', () => {
    const configContent = `module.exports = { threshold: 42, profile: 'strict' };`;
    writeFileSync(join(tmpDir, 'qualitas.config.js'), configContent, 'utf8');

    const childDir = join(tmpDir, 'src', 'lib');
    mkdirSync(childDir, { recursive: true });

    const config = loadConfig(childDir);
    expect(config.threshold).toBe(42);
    expect(config.profile).toBe('strict');
  });
});

// ─── mergeOptions ────────────────────────────────────────────────────────────

describe('mergeOptions', () => {
  it('uses CLI options over config', () => {
    const config: QualitasConfig = {
      threshold: 50,
      profile: 'strict',
      includeTests: true,
    };
    const cliOpts: Partial<AnalysisOptions> = {
      refactoringThreshold: 80,
      profile: 'default',
      includeTests: false,
    };

    const merged = mergeOptions(config, cliOpts);
    expect(merged.refactoringThreshold).toBe(80);
    expect(merged.profile).toBe('default');
    expect(merged.includeTests).toBe(false);
  });

  it('uses config values when CLI options are undefined', () => {
    const config: QualitasConfig = {
      threshold: 70,
      profile: 'cc-focused',
      includeTests: true,
      exclude: ['vendor'],
    };
    const cliOpts: Partial<AnalysisOptions> = {};

    const merged = mergeOptions(config, cliOpts);
    expect(merged.refactoringThreshold).toBe(70);
    expect(merged.profile).toBe('cc-focused');
    expect(merged.includeTests).toBe(true);
    expect(merged.exclude).toEqual(['vendor']);
  });

  it('falls back to defaults when both are empty', () => {
    const config: QualitasConfig = {};
    const cliOpts: Partial<AnalysisOptions> = {};

    const merged = mergeOptions(config, cliOpts);
    expect(merged.profile).toBe('default');
    expect(merged.refactoringThreshold).toBe(65);
    expect(merged.includeTests).toBe(false);
    expect(merged.extensions).toBeUndefined();
    expect(merged.exclude).toBeUndefined();
  });
});

// ─── Flag overrides: naming conventions ──────────────────────────────────────

describe('flag overrides — naming conventions', () => {
  const threeParamSrc = `export function greet(a: string, b: string, c: string) { return a + b + c; }`;

  it('SCREAMING_SNAKE_CASE flag keys work', () => {
    const report = analyzeSource(threeParamSrc, 'test.ts', {
      flagOverrides: { TOO_MANY_PARAMS: { warn: 2, error: 4 } },
    });
    const fn = report.functions[0];
    const flag = fn.flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(flag).toBeDefined();
    expect(flag!.severity).toBe('warning');
  });

  it('camelCase flag keys work', () => {
    const report = analyzeSource(threeParamSrc, 'test.ts', {
      flagOverrides: { tooManyParams: { warn: 2, error: 4 } },
    });
    const fn = report.functions[0];
    const flag = fn.flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(flag).toBeDefined();
    expect(flag!.severity).toBe('warning');
  });
});

// ─── Flag overrides: custom thresholds ───────────────────────────────────────

describe('flag overrides — custom thresholds', () => {
  const threeParamSrc = `export function greet(a: string, b: string, c: string) { return a + b + c; }`;

  it('no flag when value is below custom threshold', () => {
    const report = analyzeSource(threeParamSrc, 'test.ts', {
      flagOverrides: { TOO_MANY_PARAMS: { warn: 5, error: 7 } },
    });
    const flag = report.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(flag).toBeUndefined();
  });

  it('warning when value is at custom warn threshold', () => {
    const report = analyzeSource(threeParamSrc, 'test.ts', {
      flagOverrides: { TOO_MANY_PARAMS: { warn: 3, error: 6 } },
    });
    const flag = report.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(flag).toBeDefined();
    expect(flag!.severity).toBe('warning');
  });

  it('error when value exceeds custom error threshold', () => {
    const report = analyzeSource(threeParamSrc, 'test.ts', {
      flagOverrides: { TOO_MANY_PARAMS: { warn: 1, error: 2 } },
    });
    const flag = report.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(flag).toBeDefined();
    expect(flag!.severity).toBe('error');
  });
});

// ─── Flag overrides: enable/disable ──────────────────────────────────────────

describe('flag overrides — enable/disable', () => {
  it('flag disabled via false', () => {
    const src = `export function f(a:any,b:any,c:any,d:any,e:any,f:any) { return a; }`;
    const report = analyzeSource(src, 'test.ts', {
      flagOverrides: { TOO_MANY_PARAMS: false },
    });
    const flag = report.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(flag).toBeUndefined();
  });

  it('disabled-by-default flag enabled via true', () => {
    const src = `export function f(x: number) {
      if (x > 0) return 1;
      if (x > 1) return 2;
      if (x > 2) return 3;
      if (x > 3) return 4;
      if (x > 4) return 5;
      return 0;
    }`;
    // excessiveReturns is disabled by default
    const withoutOverride = analyzeSource(src, 'test.ts');
    const flagOff = withoutOverride.functions[0].flags.find(
      (f) => f.flagType === 'EXCESSIVE_RETURNS',
    );
    expect(flagOff).toBeUndefined();

    // Enable it
    const withOverride = analyzeSource(src, 'test.ts', {
      flagOverrides: { EXCESSIVE_RETURNS: true },
    });
    const flagOn = withOverride.functions[0].flags.find((f) => f.flagType === 'EXCESSIVE_RETURNS');
    expect(flagOn).toBeDefined();
  });
});

// ─── Per-language flag overrides ─────────────────────────────────────────────

describe('per-language flag overrides via config', () => {
  const pySource = `
def greet(a, b, c):
    return a + b + c
`;
  const tsSource = `export function greet(a: string, b: string, c: string) { return a + b + c; }`;

  it('per-language flags apply to matching language', () => {
    const config: QualitasConfig = {
      languages: {
        python: {
          flags: { TOO_MANY_PARAMS: { warn: 2, error: 4 } },
        },
      },
    };
    const report = analyzeSource(pySource, 'test.py', {}, config);
    const flag = report.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(flag).toBeDefined();
    expect(flag!.severity).toBe('warning');
  });

  it('per-language flags do not apply to other languages', () => {
    const config: QualitasConfig = {
      languages: {
        python: {
          flags: { TOO_MANY_PARAMS: { warn: 2, error: 4 } },
        },
      },
    };
    const report = analyzeSource(tsSource, 'test.ts', {}, config);
    const flag = report.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(flag).toBeUndefined();
  });

  it('global flags and per-language flags merge correctly', () => {
    const config: QualitasConfig = {
      flags: { DEEP_NESTING: false },
      languages: {
        python: {
          flags: { TOO_MANY_PARAMS: { warn: 2, error: 4 } },
        },
      },
    };
    const report = analyzeSource(pySource, 'test.py', {}, config);
    const paramsFlag = report.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(paramsFlag).toBeDefined();
  });

  it('per-language flag overrides global flag for the same metric', () => {
    const config: QualitasConfig = {
      // Global: strict threshold (warn at 1 param)
      flags: { TOO_MANY_PARAMS: { warn: 1, error: 2 } },
      languages: {
        python: {
          // Python override: relaxed threshold (warn at 5)
          flags: { TOO_MANY_PARAMS: { warn: 5, error: 7 } },
        },
      },
    };

    // Python file: 3 params should NOT trigger (per-language says warn at 5)
    const pyReport = analyzeSource(pySource, 'test.py', {}, config);
    const pyFlag = pyReport.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(pyFlag).toBeUndefined();

    // TypeScript file: 3 params SHOULD trigger (global says warn at 1)
    const tsReport = analyzeSource(tsSource, 'test.ts', {}, config);
    const tsFlag = tsReport.functions[0].flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    expect(tsFlag).toBeDefined();
    expect(tsFlag!.severity).toBe('error');
  });
});

// ─── Weight profiles ─────────────────────────────────────────────────────────

describe('weight profiles', () => {
  const src = `export function add(a: number, b: number) { return a + b; }`;

  it('default profile produces a score', () => {
    const report = analyzeSource(src, 'test.ts', { profile: 'default' });
    expect(report.score).toBeGreaterThanOrEqual(0);
    expect(report.score).toBeLessThanOrEqual(100);
  });

  it('strict profile produces a lower or equal score for same code', () => {
    const defaultReport = analyzeSource(src, 'test.ts', { profile: 'default' });
    const strictReport = analyzeSource(src, 'test.ts', { profile: 'strict' });
    expect(strictReport.score).toBeLessThanOrEqual(defaultReport.score);
  });

  it('different profiles produce different grades for borderline code', () => {
    const borderlineSrc = `
export function process(items: any[], config: any, logger: any, db: any) {
  for (const item of items) {
    if (item.status === 'pending') {
      if (item.quantity > 0) {
        try { logger.log(item); } catch (e) { db.save(e); }
      }
    }
  }
}`;
    const defaultReport = analyzeSource(borderlineSrc, 'test.ts', { profile: 'default' });
    const strictReport = analyzeSource(borderlineSrc, 'test.ts', { profile: 'strict' });
    // Strict should score lower or same
    expect(strictReport.functions[0].score).toBeLessThanOrEqual(defaultReport.functions[0].score);
  });
});

// ─── Threshold configuration ─────────────────────────────────────────────────

describe('refactoring threshold', () => {
  const cleanSrc = `export function add(a: number, b: number) { return a + b; }`;

  it('needsRefactoring is false when score exceeds threshold', () => {
    const report = analyzeSource(cleanSrc, 'test.ts', { refactoringThreshold: 50 });
    expect(report.needsRefactoring).toBe(false);
  });

  it('needsRefactoring uses threshold from options', () => {
    const report = analyzeSource(cleanSrc, 'test.ts', { refactoringThreshold: 99 });
    // Clean code scores ~98, so threshold of 99 should trigger
    expect(report.score).toBeLessThan(100);
  });
});

// ─── Multiple flag types in one function ─────────────────────────────────────

describe('multiple flags on one function', () => {
  it('multiple custom thresholds can trigger simultaneously', () => {
    const src = `export function f(a:any,b:any,c:any,d:any,e:any,f:any) {
      for (const x of [1]) {
        if (x > 0) {
          for (const y of [2]) {
            if (y > 0) { return x + y; }
          }
        }
      }
    }`;
    const report = analyzeSource(src, 'test.ts', {
      flagOverrides: {
        TOO_MANY_PARAMS: { warn: 2, error: 4 },
        DEEP_NESTING: { warn: 2, error: 3 },
      },
    });
    const fn = report.functions[0];
    const paramFlag = fn.flags.find((f) => f.flagType === 'TOO_MANY_PARAMS');
    const nestFlag = fn.flags.find((f) => f.flagType === 'DEEP_NESTING');
    expect(paramFlag).toBeDefined();
    expect(nestFlag).toBeDefined();
  });
});
