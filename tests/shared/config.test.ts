/**
 * Config loading and merging tests.
 *
 * These tests verify that loadConfig walks up directories to find
 * qualitas.config.js, and that mergeOptions correctly prioritises
 * CLI options > config file > built-in defaults.
 */

import { mkdtempSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { loadConfig, mergeOptions } from '../../js/config.js';
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
    // Write a config file in the temp root
    const configContent = `module.exports = { threshold: 42, profile: 'strict' };`;
    writeFileSync(join(tmpDir, 'qualitas.config.js'), configContent, 'utf8');

    // Create a nested child directory
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
    // extensions and exclude should be undefined (caller uses its own defaults)
    expect(merged.extensions).toBeUndefined();
    expect(merged.exclude).toBeUndefined();
  });
});
