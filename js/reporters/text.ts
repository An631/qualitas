import pc from 'picocolors';
import type {
  ClassQualityReport,
  FileQualityReport,
  FunctionQualityReport,
  Grade,
  ProjectQualityReport,
  RefactoringFlag,
} from '../types.js';

export interface TextReporterOptions {
  verbose?: boolean;
  flaggedOnly?: boolean;
  scope?: 'function' | 'class' | 'file' | 'module';
}

// ─── Grade colors ─────────────────────────────────────────────────────────────

function gradeColor(grade: Grade, text: string): string {
  switch (grade) {
    case 'A':
      return pc.green(text);
    case 'B':
      return pc.cyan(text);
    case 'C':
      return pc.yellow(text);
    case 'D':
      return pc.red(text);
    case 'F':
      return pc.bgRed(pc.white(text));
  }
}

function scoreBar(score: number): string {
  const filled = Math.round(score / 10);
  const bar = '█'.repeat(filled) + '░'.repeat(10 - filled);
  return gradeColor(scoreToGrade(score), bar);
}

function scoreToGrade(score: number): Grade {
  if (score >= 80) return 'A';
  if (score >= 65) return 'B';
  if (score >= 50) return 'C';
  if (score >= 35) return 'D';
  return 'F';
}

// ─── Flag rendering ───────────────────────────────────────────────────────────

function renderFlag(flag: RefactoringFlag, indent = '    '): string {
  const sev = flag.severity === 'error' ? pc.red('[error]') : pc.yellow('[warn] ');
  return `${indent}${sev} ${flag.message}\n${indent}       ${pc.dim('→')} ${flag.suggestion}`;
}

function renderFlagList(flags: RefactoringFlag[]): string[] {
  if (flags.length === 0) return [];
  const lines: string[] = [];
  lines.push('    ' + pc.dim('Flags:'));
  for (const flag of flags) {
    lines.push(renderFlag(flag));
  }
  return lines;
}

// ─── Function row ─────────────────────────────────────────────────────────────

function renderFunctionHeader(fn: FunctionQualityReport): string {
  const icon = fn.needsRefactoring ? pc.red('✗') : pc.green('✓');
  const grade = gradeColor(fn.grade, `[${fn.grade}]`);
  const score = fn.needsRefactoring
    ? pc.red(`score: ${fn.score.toFixed(0)}`)
    : pc.green(`score: ${fn.score.toFixed(0)}`);

  let line = `  ${icon} ${pc.bold(fn.name)}()  ${grade}  ${score}`;

  if (fn.needsRefactoring) {
    line += pc.dim('  ← needs refactoring');
  }

  return line;
}

function renderVerboseMetrics(fn: FunctionQualityReport): string {
  const m = fn.metrics;
  return (
    `      CFC: ${m.cognitiveFlow.score} (${scoreToGrade(100 - m.cognitiveFlow.score * 4)})  ` +
    `DCI: ${m.dataComplexity.difficulty.toFixed(1)} (${scoreToGrade(100 - m.dataComplexity.difficulty)})  ` +
    `IRC: ${m.identifierReference.totalIrc.toFixed(1)}  ` +
    `Params: ${m.structural.parameterCount}  ` +
    `LOC: ${m.structural.loc}`
  );
}

function renderFunction(fn: FunctionQualityReport, opts: TextReporterOptions): string {
  const lines = [renderFunctionHeader(fn)];

  if (opts.verbose) {
    lines.push(renderVerboseMetrics(fn));
  }

  lines.push(...renderFlagList(fn.flags));

  return lines.join('\n');
}

// ─── File report ──────────────────────────────────────────────────────────────

function renderFileSummaryLine(report: FileQualityReport): string {
  const flagCount = report.flaggedFunctionCount;
  const total = report.functionCount;
  return (
    `File: ${gradeColor(report.grade, `${report.grade} — ${report.score.toFixed(1)}`)}` +
    (flagCount > 0
      ? pc.red(`  — ${flagCount} of ${total} function(s) need refactoring`)
      : pc.green(`  — all ${total} function(s) within thresholds`))
  );
}

function renderFileHeader(report: FileQualityReport): string[] {
  const gradeStr = gradeColor(report.grade, report.grade);
  return [
    '',
    pc.bold(`qualitas-ts: ${report.filePath}`),
    `${scoreBar(report.score)}  score: ${report.score.toFixed(1)}  grade: ${gradeStr}`,
    '',
  ];
}

function renderClassScopeSection(report: FileQualityReport, opts: TextReporterOptions): string[] {
  const lines: string[] = [];
  for (const cls of report.classes) {
    if (opts.flaggedOnly && !cls.needsRefactoring) continue;
    lines.push(
      `  ${pc.cyan(`class ${cls.name}`)}  ${gradeColor(cls.grade, cls.grade)}  score: ${cls.score.toFixed(1)}`,
    );
    lines.push(...renderFlagList(cls.flags));
  }
  return lines;
}

function renderFunctionScopeSection(
  report: FileQualityReport,
  opts: TextReporterOptions,
): string[] {
  const fns = opts.flaggedOnly
    ? report.functions.filter((f) => f.needsRefactoring)
    : report.functions;

  const lines = fns.map((fn) => renderFunction(fn, opts));

  for (const cls of report.classes) {
    lines.push(...renderClassMethods(cls, opts));
  }

  return lines;
}

function renderClassMethods(cls: ClassQualityReport, opts: TextReporterOptions): string[] {
  const methods = opts.flaggedOnly ? cls.methods.filter((m) => m.needsRefactoring) : cls.methods;

  if (opts.flaggedOnly && methods.length === 0) return [];

  return [
    '',
    `  ${pc.cyan(`class ${cls.name}`)}  ${gradeColor(cls.grade, cls.grade)}  score: ${cls.score.toFixed(1)}`,
    ...methods.map((m) => renderFunction(m, opts)),
  ];
}

export function renderFileReport(
  report: FileQualityReport,
  opts: TextReporterOptions = {},
): string {
  const lines = renderFileHeader(report);
  const scope = opts.scope ?? 'function';

  if (scope === 'file') {
    lines.push(renderFileSummaryLine(report));
    return lines.join('\n');
  }

  if (scope === 'class') {
    lines.push(...renderClassScopeSection(report, opts));
  } else {
    lines.push(...renderFunctionScopeSection(report, opts));
  }

  lines.push('');
  lines.push(renderFileSummaryLine(report));

  return lines.join('\n');
}

// ─── Project report ───────────────────────────────────────────────────────────

function renderProjectHeader(report: ProjectQualityReport): string[] {
  const s = report.summary;
  return [
    '',
    pc.bold(`qualitas-ts project: ${report.dirPath}`),
    `${scoreBar(report.score)}  score: ${report.score.toFixed(1)}  grade: ${gradeColor(report.grade, report.grade)}`,
    '',
    `  ${s.totalFiles} files  |  ${s.totalFunctions} functions  |  ` +
      pc.red(`${s.flaggedFunctions} need refactoring`),
    `  Grades: ${pc.green('A:' + s.gradeDistribution.a)}  ${pc.cyan('B:' + s.gradeDistribution.b)}  ` +
      `${pc.yellow('C:' + s.gradeDistribution.c)}  ${pc.red('D:' + s.gradeDistribution.d)}  ` +
      `${pc.bgRed(pc.white('F:' + s.gradeDistribution.f))}`,
  ];
}

function renderWorstFunctionsSection(worstFunctions: FunctionQualityReport[]): string[] {
  if (worstFunctions.length === 0) return [];
  const lines: string[] = ['', pc.bold('  Worst functions:')];
  for (const fn of worstFunctions.slice(0, 5)) {
    lines.push(
      `    ${pc.red('✗')} ${fn.location.file}  ${pc.bold(fn.name)}()  score: ${fn.score.toFixed(0)}  ${gradeColor(fn.grade, fn.grade)}`,
    );
  }
  return lines;
}

function renderProjectFileDetails(
  report: ProjectQualityReport,
  opts: TextReporterOptions,
): string[] {
  if (!opts.verbose && opts.flaggedOnly) return [];
  const lines: string[] = [''];
  for (const file of report.files) {
    if (opts.flaggedOnly && !file.needsRefactoring) continue;
    lines.push(renderFileReport(file, opts));
  }
  return lines;
}

export function renderProjectReport(
  report: ProjectQualityReport,
  opts: TextReporterOptions = {},
): string {
  const lines = renderProjectHeader(report);
  const scope = opts.scope ?? 'function';

  if (scope === 'module') {
    return lines.join('\n');
  }

  if (scope === 'function') {
    lines.push(...renderWorstFunctionsSection(report.worstFunctions));
  }

  lines.push(...renderProjectFileDetails(report, opts));

  return lines.join('\n');
}
