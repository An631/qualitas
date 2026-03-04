/**
 * Reporter output tests.
 *
 * These tests verify that the JSON, Markdown, and text reporters produce
 * well-formed output containing the expected sections and data.
 */

import type {
  FileQualityReport,
  ProjectQualityReport,
  Grade,
} from '../../js/types.js';

let analyzeSource: typeof import('../../js/index').analyzeSource;
let renderFileReport: typeof import('../../js/reporters/text').renderFileReport;
let renderProjectReport: typeof import('../../js/reporters/text').renderProjectReport;
let renderJsonReport: typeof import('../../js/reporters/json').renderJsonReport;
let renderMarkdownReport: typeof import('../../js/reporters/markdown').renderMarkdownReport;

beforeAll(async () => {
  try {
    const indexMod = await import('../../js/index.js');
    analyzeSource = indexMod.analyzeSource;
  } catch (err) {
    console.warn(
      'Native binding not available, skipping integration tests:',
      (err as Error).message,
    );
  }

  const textMod = await import('../../js/reporters/text.js');
  renderFileReport = textMod.renderFileReport;
  renderProjectReport = textMod.renderProjectReport;

  const jsonMod = await import('../../js/reporters/json.js');
  renderJsonReport = jsonMod.renderJsonReport;

  const mdMod = await import('../../js/reporters/markdown.js');
  renderMarkdownReport = mdMod.renderMarkdownReport;
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

// ─── Shared source snippets ──────────────────────────────────────────────────

const CLEAN_SRC = `
export function add(a: number, b: number): number { return a + b; }
export function multiply(x: number, y: number): number { return x * y; }
`;

const COMPLEX_SRC = `
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

// ─── JSON reporter ───────────────────────────────────────────────────────────

describe('renderJsonReport', () => {
  it(
    'produces valid JSON with score, grade, and functions',
    skipIfNoBinding(() => {
      const report = analyzeSource(CLEAN_SRC, 'clean.ts');
      const output = renderJsonReport(report);
      const parsed = JSON.parse(output) as FileQualityReport;
      expect(typeof parsed.score).toBe('number');
      expect(['A', 'B', 'C', 'D', 'F'] as Grade[]).toContain(parsed.grade);
      expect(Array.isArray(parsed.functions)).toBe(true);
      expect(parsed.functions.length).toBeGreaterThan(0);
    }),
  );
});

// ─── Markdown reporter ───────────────────────────────────────────────────────

describe('renderMarkdownReport', () => {
  it(
    'contains function table with expected headers',
    skipIfNoBinding(() => {
      const report = analyzeSource(CLEAN_SRC, 'clean.ts');
      const output = renderMarkdownReport(report);
      expect(output).toContain('| Function |');
      expect(output).toContain('| Score |');
      expect(output).toContain('| Grade |');
      expect(output).toContain('| CFC |');
      expect(output).toContain('| DCI |');
      expect(output).toContain('| IRC |');
      expect(output).toContain('| Params |');
      expect(output).toContain('| LOC |');
    }),
  );
});

// ─── Text file reporter ──────────────────────────────────────────────────────

describe('renderFileReport', () => {
  it(
    'verbose mode shows metric labels',
    skipIfNoBinding(() => {
      const report = analyzeSource(CLEAN_SRC, 'verbose.ts');
      const output = renderFileReport(report, { verbose: true });
      expect(output).toContain('CFC:');
      expect(output).toContain('DCI:');
      expect(output).toContain('IRC:');
    }),
  );

  it(
    'flaggedOnly hides clean functions',
    skipIfNoBinding(() => {
      const report = analyzeSource(CLEAN_SRC, 'clean.ts');
      // Clean code should have no flagged functions
      const output = renderFileReport(report, { flaggedOnly: true });
      // The function names should not appear since none are flagged
      for (const fn of report.functions) {
        if (!fn.needsRefactoring) {
          expect(output).not.toMatch(new RegExp(`✓.*${fn.name}`));
        }
      }
    }),
  );
});

// ─── Text project reporter ───────────────────────────────────────────────────

describe('renderProjectReport', () => {
  it(
    'includes grade distribution counts',
    skipIfNoBinding(() => {
      // Build a minimal ProjectQualityReport from two file reports
      const cleanReport = analyzeSource(CLEAN_SRC, 'clean.ts');
      const complexReport = analyzeSource(COMPLEX_SRC, 'complex.ts');

      const allFunctions = [
        ...cleanReport.functions,
        ...complexReport.functions,
      ];

      const dist = { a: 0, b: 0, c: 0, d: 0, f: 0 };
      for (const f of allFunctions) {
        dist[f.grade.toLowerCase() as keyof typeof dist]++;
      }

      const projectReport: ProjectQualityReport = {
        dirPath: '/test',
        score: 75,
        grade: 'B',
        needsRefactoring: false,
        files: [cleanReport, complexReport],
        summary: {
          totalFiles: 2,
          totalFunctions: allFunctions.length,
          totalClasses: 0,
          flaggedFiles: 1,
          flaggedFunctions: allFunctions.filter((f) => f.needsRefactoring).length,
          averageScore: 75,
          gradeDistribution: dist,
        },
        worstFunctions: [...allFunctions].sort((a, b) => a.score - b.score).slice(0, 5),
      };

      const output = renderProjectReport(projectReport);
      expect(output).toContain('A:');
      expect(output).toContain('B:');
      expect(output).toContain('C:');
      expect(output).toContain('D:');
      expect(output).toContain('F:');
    }),
  );
});
