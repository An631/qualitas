#!/usr/bin/env node
import { program } from 'commander';
import { analyzeFile, analyzeProject } from './index.js';
import { renderFileReport, renderProjectReport } from './reporters/text.js';
import { renderJsonReport } from './reporters/json.js';
import { renderMarkdownReport, renderMarkdownProjectReport } from './reporters/markdown.js';
import { statSync } from 'node:fs';
import { resolve, basename } from 'node:path';
import type {
  AnalysisOptions,
  FileQualityReport,
  ProfileName,
  ProjectQualityReport,
} from './types.js';
import { loadConfig } from './config.js';

program
  .name('qualitas')
  .description(
    'TypeScript/JavaScript code quality measurement — Quality Score 0–100 (higher = better)',
  )
  .version('0.1.0');

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
  .action(async (targetPath: string, opts) => {
    const resolvedPath = resolve(targetPath);
    const config = loadConfig(resolvedPath);

    const options: AnalysisOptions = {
      profile: (opts.profile !== 'default'
        ? opts.profile
        : (config.profile ?? 'default')) as ProfileName,
      refactoringThreshold:
        opts.threshold !== '65' ? parseFloat(opts.threshold) : (config.threshold ?? 65),
      includeTests: opts.includeTests ?? config.includeTests ?? false,
    };

    // Resolve format: CLI > config > default
    const format = opts.format !== 'text' ? opts.format : (config.format ?? 'text');

    // Derive internal flags from format preset
    const verbose = format === 'detail';
    const flaggedOnly = format === 'flagged';
    const scope = 'function';

    const threshold = options.refactoringThreshold ?? 65;
    let belowThreshold = false;

    try {
      let stat;
      try {
        stat = statSync(targetPath);
      } catch {
        console.error(`qualitas: path not found: ${targetPath}`);
        process.exit(2);
      }

      if (stat.isDirectory()) {
        const report = await analyzeProject(targetPath, options);
        belowThreshold =
          report.score < threshold ||
          report.files.some((f) => f.functions.some((fn) => fn.score < threshold));

        if (format === 'json') {
          console.log(renderJsonReport(report));
        } else if (format === 'markdown') {
          console.log(renderMarkdownProjectReport(report));
        } else if (format === 'compact') {
          console.log(renderCompactProject(report));
        } else {
          console.log(
            renderProjectReport(report, {
              verbose,
              flaggedOnly,
              scope,
            }),
          );
        }
      } else {
        const report = await analyzeFile(targetPath, options);
        belowThreshold =
          report.score < threshold || report.functions.some((fn) => fn.score < threshold);

        if (format === 'json') {
          console.log(renderJsonReport(report));
        } else if (format === 'markdown') {
          console.log(renderMarkdownReport(report));
        } else if (format === 'compact') {
          console.log(renderCompactFile(report));
        } else {
          console.log(
            renderFileReport(report, {
              verbose,
              flaggedOnly,
              scope,
            }),
          );
        }
      }

      process.exit(belowThreshold ? 1 : 0);
    } catch (err) {
      console.error(`qualitas error: ${(err as Error).message}`);
      process.exit(2);
    }
  });

program.parse();

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

function renderCompactLine(filePath: string, score: number, grade: string, flagCount: number): string {
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
