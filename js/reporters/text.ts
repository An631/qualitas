import pc from 'picocolors';
import type {
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
    case 'A': return pc.green(text);
    case 'B': return pc.cyan(text);
    case 'C': return pc.yellow(text);
    case 'D': return pc.red(text);
    case 'F': return pc.bgRed(pc.white(text));
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

// ─── Function row ─────────────────────────────────────────────────────────────

function renderFunction(fn: FunctionQualityReport, opts: TextReporterOptions): string {
  const icon = fn.needsRefactoring ? pc.red('✗') : pc.green('✓');
  const grade = gradeColor(fn.grade, `[${fn.grade}]`);
  const score = fn.needsRefactoring
    ? pc.red(`score: ${fn.score.toFixed(0)}`)
    : pc.green(`score: ${fn.score.toFixed(0)}`);

  let line = `  ${icon} ${pc.bold(fn.name)}()  ${grade}  ${score}`;

  if (fn.needsRefactoring) {
    line += pc.dim('  ← needs refactoring');
  }

  const lines = [line];

  if (opts.verbose) {
    const m = fn.metrics;
    lines.push(
      `      CFC: ${m.cognitiveFlow.score} (${scoreToGrade(100 - m.cognitiveFlow.score * 4)})  ` +
      `DCI: ${m.dataComplexity.difficulty.toFixed(1)} (${scoreToGrade(100 - m.dataComplexity.difficulty)})  ` +
      `IRC: ${m.identifierReference.totalIrc.toFixed(1)}  ` +
      `Params: ${m.structural.parameterCount}  ` +
      `LOC: ${m.structural.loc}`
    );
  }

  if (fn.flags.length > 0) {
    lines.push('    ' + pc.dim('Flags:'));
    for (const flag of fn.flags) {
      lines.push(renderFlag(flag));
    }
  }

  return lines.join('\n');
}

// ─── File report ──────────────────────────────────────────────────────────────

export function renderFileReport(report: FileQualityReport, opts: TextReporterOptions = {}): string {
  const lines: string[] = [];
  const gradeStr = gradeColor(report.grade, report.grade);

  lines.push('');
  lines.push(pc.bold(`qualitas-ts: ${report.filePath}`));
  lines.push(`${scoreBar(report.score)}  score: ${report.score.toFixed(1)}  grade: ${gradeStr}`);
  lines.push('');

  const fns = opts.flaggedOnly
    ? report.functions.filter(f => f.needsRefactoring)
    : report.functions;

  for (const fn of fns) {
    lines.push(renderFunction(fn, opts));
  }

  for (const cls of report.classes) {
    const classFns = opts.flaggedOnly
      ? cls.methods.filter(m => m.needsRefactoring)
      : cls.methods;

    if (opts.flaggedOnly && classFns.length === 0) continue;

    lines.push('');
    lines.push(`  ${pc.cyan(`class ${cls.name}`)}  ${gradeColor(cls.grade, cls.grade)}  score: ${cls.score.toFixed(1)}`);
    for (const m of classFns) {
      lines.push(renderFunction(m, opts));
    }
  }

  lines.push('');
  const flagCount = report.flaggedFunctionCount;
  const total = report.functionCount;
  lines.push(
    `File: ${gradeColor(report.grade, `${report.grade} — ${report.score.toFixed(1)}`)}` +
    (flagCount > 0
      ? pc.red(`  — ${flagCount} of ${total} function(s) need refactoring`)
      : pc.green(`  — all ${total} function(s) within thresholds`))
  );

  return lines.join('\n');
}

// ─── Project report ───────────────────────────────────────────────────────────

export function renderProjectReport(report: ProjectQualityReport, opts: TextReporterOptions = {}): string {
  const lines: string[] = [];

  lines.push('');
  lines.push(pc.bold(`qualitas-ts project: ${report.dirPath}`));
  lines.push(`${scoreBar(report.score)}  score: ${report.score.toFixed(1)}  grade: ${gradeColor(report.grade, report.grade)}`);
  lines.push('');

  const s = report.summary;
  lines.push(
    `  ${s.totalFiles} files  |  ${s.totalFunctions} functions  |  ` +
    pc.red(`${s.flaggedFunctions} need refactoring`)
  );
  lines.push(
    `  Grades: ${pc.green('A:' + s.gradeDistribution.a)}  ${pc.cyan('B:' + s.gradeDistribution.b)}  ` +
    `${pc.yellow('C:' + s.gradeDistribution.c)}  ${pc.red('D:' + s.gradeDistribution.d)}  ` +
    `${pc.bgRed(pc.white('F:' + s.gradeDistribution.f))}`
  );

  if (report.worstFunctions.length > 0) {
    lines.push('');
    lines.push(pc.bold('  Worst functions:'));
    for (const fn of report.worstFunctions.slice(0, 5)) {
      lines.push(`    ${pc.red('✗')} ${fn.location.file}  ${pc.bold(fn.name)}()  score: ${fn.score.toFixed(0)}  ${gradeColor(fn.grade, fn.grade)}`);
    }
  }

  if (opts.verbose || !opts.flaggedOnly) {
    lines.push('');
    for (const file of report.files) {
      if (opts.flaggedOnly && !file.needsRefactoring) continue;
      lines.push(renderFileReport(file, opts));
    }
  }

  return lines.join('\n');
}
