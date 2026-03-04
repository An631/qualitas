import { existsSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';

import type { AnalysisOptions, QualitasConfig } from './types.js';

const CONFIG_FILENAME = 'qualitas.config.js';

/**
 * Walk up directories from `startDir` looking for `qualitas.config.js`.
 * Returns the parsed config object, or `{}` if no config file is found.
 */
export function loadConfig(startDir: string): QualitasConfig {
  try {
    const configPath = findConfigFile(startDir);
    if (!configPath) return {};
    const loaded = require(configPath);
    return (loaded && typeof loaded === 'object' ? loaded : {}) as QualitasConfig;
  } catch {
    return {};
  }
}

function findConfigFile(startDir: string): string | null {
  let dir = resolve(startDir);
  while (true) {
    const candidate = join(dir, CONFIG_FILENAME);
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

/**
 * Merge a loaded config with CLI options into a complete `AnalysisOptions`.
 * Priority: CLI options > config file > defaults.
 */
export function mergeOptions(
  config: QualitasConfig,
  cliOpts: Partial<AnalysisOptions>,
): AnalysisOptions {
  return {
    profile: cliOpts.profile ?? config.profile ?? 'default',
    refactoringThreshold: cliOpts.refactoringThreshold ?? config.threshold ?? 65,
    includeTests: cliOpts.includeTests ?? config.includeTests ?? false,
    extensions: cliOpts.extensions ?? config.extensions,
    exclude: cliOpts.exclude ?? config.exclude,
    weights: cliOpts.weights ?? config.weights,
  };
}
