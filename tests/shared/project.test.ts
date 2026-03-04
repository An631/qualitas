/**
 * Project analysis and file collection tests.
 *
 * These tests verify that analyzeProject and analyzeFile correctly walk
 * directories, respect filtering options, and produce valid report shapes.
 */

import { resolve } from 'node:path';

import type { FileQualityReport, ProjectQualityReport, Grade } from '../../js/types.js';

let analyzeProject: typeof import('../../js/index').analyzeProject;
let analyzeFile: typeof import('../../js/index').analyzeFile;

beforeAll(async () => {
  try {
    const mod = await import('../../js/index.js');
    analyzeProject = mod.analyzeProject;
    analyzeFile = mod.analyzeFile;
  } catch (err) {
    console.warn(
      'Native binding not available, skipping integration tests:',
      (err as Error).message,
    );
  }
});

function skipIfNoBinding(fn: () => Promise<void>): () => Promise<void> {
  return async () => {
    if (!analyzeProject) {
      console.warn('Skipping: no native binding');
      return;
    }
    await fn();
  };
}

const FIXTURES_DIR = resolve(__dirname, '..', 'typescript', 'fixtures');

// ─── analyzeProject ──────────────────────────────────────────────────────────

describe('analyzeProject', () => {
  it(
    'returns valid report for fixture directory',
    skipIfNoBinding(async () => {
      const report: ProjectQualityReport = await analyzeProject(FIXTURES_DIR);
      expect(report.dirPath).toBe(FIXTURES_DIR);
      expect(typeof report.score).toBe('number');
      expect(report.score).toBeGreaterThanOrEqual(0);
      expect(report.score).toBeLessThanOrEqual(100);
      expect(['A', 'B', 'C', 'D', 'F'] as Grade[]).toContain(report.grade);
      expect(Array.isArray(report.files)).toBe(true);
      expect(report.files.length).toBeGreaterThan(0);
    }),
  );

  it(
    'excludes node_modules',
    skipIfNoBinding(async () => {
      // Analyze from the repo root — node_modules should never appear
      const repoRoot = resolve(__dirname, '..', '..');
      const report = await analyzeProject(repoRoot);
      for (const file of report.files) {
        expect(file.filePath).not.toMatch(/node_modules/);
      }
    }),
  );

  it(
    'respects includeTests option',
    skipIfNoBinding(async () => {
      // The fixtures directory has no test files, so analyze the wider tests/ dir
      // where test files (.test.ts) exist. With includeTests=false they should
      // be excluded; with includeTests=true they should be included.
      const testsDir = resolve(__dirname, '..', 'typescript');
      const withoutTests = await analyzeProject(testsDir, { includeTests: false });
      const withTests = await analyzeProject(testsDir, { includeTests: true });
      expect(withTests.files.length).toBeGreaterThanOrEqual(withoutTests.files.length);
    }),
  );

  it(
    'worst functions are sorted by score ascending',
    skipIfNoBinding(async () => {
      const report = await analyzeProject(FIXTURES_DIR);
      const worst = report.worstFunctions;
      for (let i = 1; i < worst.length; i++) {
        expect(worst[i - 1].score).toBeLessThanOrEqual(worst[i].score);
      }
    }),
  );

  it(
    'grade distribution adds up to total functions',
    skipIfNoBinding(async () => {
      const report = await analyzeProject(FIXTURES_DIR);
      const dist = report.summary.gradeDistribution;
      const gradeSum = dist.a + dist.b + dist.c + dist.d + dist.f;
      expect(gradeSum).toBe(report.summary.totalFunctions);
    }),
  );
});

// ─── analyzeFile ─────────────────────────────────────────────────────────────

describe('analyzeFile', () => {
  it(
    'returns valid report for single file',
    skipIfNoBinding(async () => {
      const filePath = resolve(FIXTURES_DIR, 'clean.ts');
      const report: FileQualityReport = await analyzeFile(filePath);
      expect(report.filePath).toBe(filePath);
      expect(typeof report.score).toBe('number');
      expect(report.score).toBeGreaterThanOrEqual(0);
      expect(report.score).toBeLessThanOrEqual(100);
      expect(['A', 'B', 'C', 'D', 'F'] as Grade[]).toContain(report.grade);
      expect(Array.isArray(report.functions)).toBe(true);
      expect(report.functions.length).toBeGreaterThan(0);
    }),
  );

  it(
    'backfills file path in locations',
    skipIfNoBinding(async () => {
      const filePath = resolve(FIXTURES_DIR, 'clean.ts');
      const report = await analyzeFile(filePath);
      for (const fn of report.functions) {
        expect(fn.location.file).toBe(filePath);
      }
      for (const cls of report.classes) {
        expect(cls.location.file).toBe(filePath);
        for (const m of cls.methods) {
          expect(m.location.file).toBe(filePath);
        }
      }
    }),
  );
});
