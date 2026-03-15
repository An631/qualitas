#!/usr/bin/env node
'use strict';

const { spawnSync } = require('child_process');
const { join } = require('path');

const PLATFORMS = {
  'win32-x64': '@qualitas/binding-win32-x64-msvc',
  'darwin-x64': '@qualitas/binding-darwin-x64',
  'darwin-arm64': '@qualitas/binding-darwin-arm64',
  'linux-x64': '@qualitas/binding-linux-x64-gnu',
  'linux-arm64': '@qualitas/binding-linux-arm64-gnu',
};

function getBinaryPath() {
  const key = `${process.platform}-${process.arch}`;
  const pkg = PLATFORMS[key];
  const ext = process.platform === 'win32' ? '.exe' : '';

  if (!pkg) {
    console.error(
      `qualitas: unsupported platform ${key}.\n` +
        `Supported: ${Object.keys(PLATFORMS).join(', ')}`,
    );
    process.exit(1);
  }

  try {
    const pkgDir = join(require.resolve(`${pkg}/package.json`), '..');
    return join(pkgDir, `qualitas${ext}`);
  } catch {
    console.error(
      `qualitas: could not find platform package "${pkg}".\n` +
        'Try reinstalling: npm install qualitas',
    );
    process.exit(1);
  }
}

const result = spawnSync(getBinaryPath(), process.argv.slice(2), {
  stdio: 'inherit',
});

if (result.error) {
  console.error(`qualitas: failed to execute binary: ${result.error.message}`);
  process.exit(1);
}

if (result.signal) {
  process.kill(process.pid, result.signal);
}

process.exit(result.status ?? 1);
