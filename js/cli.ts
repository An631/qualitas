#!/usr/bin/env node
import { program } from 'commander';
import { analyzeFile, analyzeProject } from './index.js';
import { renderFileReport, renderProjectReport } from './reporters/text.js';
import { renderJsonReport } from './reporters/json.js';
import { renderMarkdownReport, renderMarkdownProjectReport } from './reporters/markdown.js';
import { type Stats, readFileSync, statSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import type {
  AnalysisOptions,
  FileQualityReport,
  ProfileName,
  ProjectQualityReport,
} from './types.js';
import { loadConfig } from './config.js';

const pkg = JSON.parse(readFileSync(join(__dirname, '..', 'package.json'), 'utf8')) as {
  version: string;
};

program
  .name('qualitas')
  .description('Code quality measurement — Quality Score 0–100 (higher = better)')
  .version(pkg.version);

program
  .argument('<path>', 'File or directory to analyze')
  .option(
    '-f, --format <format>',
    'Output format: text | json | markdown | summary | compact | detail | flagged',
    'text',
  )
  .option(
    '-p, --profile <profile>',
    'Weight profile: default | cc-focused | data-focused | strict',
    'default',
  )
  .option('-t, --threshold <number>', 'Exit code 1 if any score is below this threshold', '65')
  .option('--include-tests', 'Include test files (*.test.ts, *.spec.ts) in analysis')
  .option('--fail-on-flags <level>', 'Fail (exit 1) if any function has flags: warn | error')
  .action(runAnalysis);

program.parse();

// ─── CLI action handler ───────────────────────────────────────────────────

interface CliOpts {
  format: string;
  profile: string;
  threshold: string;
  includeTests?: boolean;
  failOnFlags?: string;
}

function buildOptions(opts: CliOpts, config: import('./types.js').QualitasConfig): AnalysisOptions {
  return {
    profile: (opts.profile !== 'default'
      ? opts.profile
      : (config.profile ?? 'default')) as ProfileName,
    refactoringThreshold:
      opts.threshold !== '65' ? parseFloat(opts.threshold) : (config.threshold ?? 65),
    includeTests: opts.includeTests ?? config.includeTests ?? false,
  };
}

function resolveFormat(opts: CliOpts, config: import('./types.js').QualitasConfig): string {
  return opts.format !== 'text' ? opts.format : (config.format ?? 'text');
}

async function runAnalysis(targetPath: string, opts: CliOpts): Promise<void> {
  const config = loadConfig(resolve(targetPath));
  const options = buildOptions(opts, config);
  const format = resolveFormat(opts, config);
  const threshold = options.refactoringThreshold ?? 65;
  const failOnFlags = opts.failOnFlags ?? config.failOnFlags;

  const check = { threshold, failOnFlags };

  try {
    const stat = safeStat(targetPath);
    const belowThreshold = stat.isDirectory()
      ? await runProjectAnalysis(targetPath, options, format, check)
      : await runFileAnalysis(targetPath, options, format, check);
    process.exit(belowThreshold ? 1 : 0);
  } catch (err) {
    console.error(`qualitas error: ${(err as Error).message}`);
    process.exit(2);
  }
}

function safeStat(targetPath: string): Stats {
  try {
    return statSync(targetPath);
  } catch {
    console.error(`qualitas: path not found: ${targetPath}`);
    throw new Error(`path not found: ${targetPath}`);
  }
}

interface CheckConfig {
  threshold: number;
  failOnFlags?: string;
}

async function runProjectAnalysis(
  targetPath: string,
  options: AnalysisOptions,
  format: string,
  check: CheckConfig,
): Promise<boolean> {
  const report = await analyzeProject(targetPath, options);
  const belowThreshold =
    report.score < check.threshold ||
    report.files.some(
      (f) =>
        f.functions.some((fn) => fn.score < check.threshold) ||
        hasFlagsAtSeverity(f.functions, check.failOnFlags),
    );

  console.log(formatProjectOutput(report, format));
  return belowThreshold;
}

async function runFileAnalysis(
  targetPath: string,
  options: AnalysisOptions,
  format: string,
  check: CheckConfig,
): Promise<boolean> {
  const report = await analyzeFile(targetPath, options);
  const belowThreshold =
    report.score < check.threshold ||
    report.functions.some((fn) => fn.score < check.threshold) ||
    hasFlagsAtSeverity(report.functions, check.failOnFlags);

  console.log(formatFileOutput(report, format));
  return belowThreshold;
}

function hasFlagsAtSeverity(
  functions: import('./types.js').FunctionQualityReport[],
  failOnFlags?: string,
): boolean {
  if (!failOnFlags) return false;
  if (failOnFlags === 'warn') {
    return functions.some((fn) => fn.flags.length > 0);
  }
  if (failOnFlags === 'error') {
    return functions.some((fn) => fn.flags.some((f) => f.severity === 'error'));
  }
  return false;
}

function formatProjectOutput(report: ProjectQualityReport, format: string): string {
  if (format === 'json') return renderJsonReport(report);
  if (format === 'markdown') return renderMarkdownProjectReport(report);
  if (format === 'compact') return renderCompactProject(report);
  return renderProjectReport(report, {
    verbose: format === 'detail',
    flaggedOnly: format === 'flagged',
    scope: 'function',
  });
}

function formatFileOutput(report: FileQualityReport, format: string): string {
  if (format === 'json') return renderJsonReport(report);
  if (format === 'markdown') return renderMarkdownReport(report);
  if (format === 'compact') return renderCompactFile(report);
  return renderFileReport(report, {
    verbose: format === 'detail',
    flaggedOnly: format === 'flagged',
    scope: 'function',
  });
}

// ─── Compact format helpers ────────────────────────────────────────────────

function shortName(filePath: string): string {
  return basename(filePath);
}

function countAllFlags(report: FileQualityReport): number {
  let n = report.flags.length;
  for (const f of report.functions) {
    n += f.flags.length;
  }
  for (const cls of report.classes) {
    for (const m of cls.methods) {
      n += m.flags.length;
    }
  }
  return n;
}

function renderCompactLine(
  filePath: string,
  score: number,
  grade: string,
  flagCount: number,
): string {
  const flagStr = flagCount > 0 ? `  ${flagCount} flags` : '';
  return `  ${grade}  ${score.toFixed(1).padStart(5)}  ${filePath}${flagStr}`;
}

function renderCompactFile(report: FileQualityReport): string {
  const flags = countAllFlags(report);
  return renderCompactLine(shortName(report.filePath), report.score, report.grade, flags);
}

function renderCompactProject(report: ProjectQualityReport): string {
  const s = report.summary;
  const files = [...report.files].sort((a, b) => a.score - b.score);
  const lines: string[] = [
    `qualitas  score: ${report.score.toFixed(1)}  grade: ${report.grade}  files: ${s.totalFiles}  flagged: ${s.flaggedFiles}`,
  ];

  for (const file of files) {
    const flags = countAllFlags(file);
    lines.push(renderCompactLine(shortName(file.filePath), file.score, file.grade, flags));
  }

  return lines.join('\n');
}
