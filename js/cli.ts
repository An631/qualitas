#!/usr/bin/env node
import { program } from 'commander';
import { analyzeFile, analyzeProject } from './index.js';
import { renderFileReport, renderProjectReport } from './reporters/text.js';
import { renderJsonReport } from './reporters/json.js';
import { renderMarkdownReport, renderMarkdownProjectReport } from './reporters/markdown.js';
import { statSync } from 'node:fs';
import { resolve } from 'node:path';
import type { AnalysisOptions, ProfileName } from './types.js';
import { loadConfig } from './config.js';

program
  .name('qualitas')
  .description(
    'TypeScript/JavaScript code quality measurement — Quality Score 0–100 (higher = better)',
  )
  .version('0.1.0');

program
  .argument('<path>', 'File or directory to analyze')
  .option('-f, --format <format>', 'Output format: text | json | markdown', 'text')
  .option(
    '-p, --profile <profile>',
    'Weight profile: default | cc-focused | data-focused | strict',
    'default',
  )
  .option('-t, --threshold <number>', 'Exit code 1 if any score is below this threshold', '65')
  .option('--flagged-only', 'Only show items needing refactoring')
  .option('--verbose', 'Show metric breakdown per function')
  .option('--scope <scope>', 'Report scope: function | class | file | module', 'function')
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

    const format = opts.format !== 'text' ? opts.format : (config.format ?? 'text');
    const scope = opts.scope !== 'function' ? opts.scope : (config.scope ?? 'function');
    const verbose = opts.verbose ?? config.verbose ?? false;
    const flaggedOnly = opts.flaggedOnly ?? config.flaggedOnly ?? false;

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
