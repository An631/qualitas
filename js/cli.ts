#!/usr/bin/env node
import { program } from 'commander';
import { analyzeFile, analyzeProject } from './index.js';
import { renderFileReport, renderProjectReport } from './reporters/text.js';
import { renderJsonReport } from './reporters/json.js';
import { renderMarkdownReport, renderMarkdownProjectReport } from './reporters/markdown.js';
import { statSync } from 'node:fs';
import type { AnalysisOptions, ProfileName } from './types.js';

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
    const options: AnalysisOptions = {
      profile: opts.profile as ProfileName,
      refactoringThreshold: parseFloat(opts.threshold),
      includeTests: opts.includeTests ?? false,
    };

    const threshold = parseFloat(opts.threshold);
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

        if (opts.format === 'json') {
          console.log(renderJsonReport(report));
        } else if (opts.format === 'markdown') {
          console.log(renderMarkdownProjectReport(report));
        } else {
          console.log(
            renderProjectReport(report, {
              verbose: opts.verbose,
              flaggedOnly: opts.flaggedOnly,
              scope: opts.scope,
            }),
          );
        }
      } else {
        const report = await analyzeFile(targetPath, options);
        belowThreshold =
          report.score < threshold || report.functions.some((fn) => fn.score < threshold);

        if (opts.format === 'json') {
          console.log(renderJsonReport(report));
        } else if (opts.format === 'markdown') {
          console.log(renderMarkdownReport(report));
        } else {
          console.log(
            renderFileReport(report, {
              verbose: opts.verbose,
              flaggedOnly: opts.flaggedOnly,
              scope: opts.scope,
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
